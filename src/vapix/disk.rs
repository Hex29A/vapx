//! Edge storage disk management: status, format, mount, unmount, SD card
//! encryption and retention (`disks/*.cgi`, Disk management and Disk properties APIs).
//!
//! Format and the encryption switch erase every recording on the card, so the
//! callers show [`impact`] first and only go on with a confirmation. The
//! passphrase of an encrypted card is a [`Secret`]: it comes from an
//! environment variable or a file, is sent in a POST body (never in a URL), and
//! is never printed or logged.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use serde_json::{json, Map, Value};
use tracing::debug;

use crate::vapix::client::VapixClient;
use crate::vapix::{params, storage};

const FORMAT: &str = "/axis-cgi/disks/format.cgi";
const JOB: &str = "/axis-cgi/disks/job.cgi";
const MOUNT: &str = "/axis-cgi/disks/mount.cgi";
const UNMOUNT: &str = "/axis-cgi/disks/unmount.cgi";
const ENABLE_ENCRYPTION: &str = "/axis-cgi/disks/enablediskencryption.cgi";
const DISABLE_ENCRYPTION: &str = "/axis-cgi/disks/disablediskencryption.cgi";
const CHANGE_PASSPHRASE: &str = "/axis-cgi/disks/changediskpassphrase.cgi";
// Retention (Disk properties API). These replace the Storage.Sn.CleanupMaxAge and
// Storage.Sn.CleanupPolicyActive parameters, which AXIS OS 13 removed.
const SET_MAX_AGE: &str = "/axis-cgi/disks/properties/setcleanupmaxage.cgi";
const SET_POLICY: &str = "/axis-cgi/disks/properties/setcleanuppolicy.cgi";
/// The longest max age the camera takes, in days. 0 means no age limit.
pub const MAX_AGE_LIMIT: u32 = 7000;

/// The card slot of a camera.
pub const DEFAULT_DISK: &str = "SD_DISK";
/// Most recordings counted when sizing up what an erase would remove.
const MAX_COUNTED: u32 = 10_000;

/// A secret that is never printed: `Debug` hides it and there is no `Display`.
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> anyhow::Result<Self> {
        if value.is_empty() {
            bail!("the passphrase is empty");
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Read a passphrase from `file` when given, otherwise from the environment
/// variable `env`. `what` names it in error messages ("passphrase", "new
/// passphrase"). The value is never part of an error. A file that others can
/// read is refused on Unix, like ssh does with a private key.
pub fn secret_from(file: Option<&Path>, env: &str, what: &str) -> anyhow::Result<Secret> {
    let raw = match file {
        Some(path) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(path)
                    .with_context(|| format!("cannot read the {what} file {}", path.display()))?
                    .permissions()
                    .mode();
                if mode & 0o077 != 0 {
                    bail!("the {what} file {} is readable by others; run chmod 600 on it", path.display());
                }
            }
            std::fs::read_to_string(path).with_context(|| format!("cannot read the {what} file {}", path.display()))?
        }
        None => std::env::var(env).map_err(|_| anyhow::anyhow!("no {what}: set the environment variable {env} or give a file"))?,
    };
    // One trailing newline comes from `echo` and editors; anything else belongs to the passphrase.
    let value = raw.strip_suffix('\n').unwrap_or(&raw);
    let value = value.strip_suffix('\r').unwrap_or(value);
    Secret::new(value.to_string()).with_context(|| format!("the {what} is empty"))
}

/// One disk as `disks/list.cgi` reports it.
#[derive(Debug, Clone)]
pub struct Disk {
    pub id: String,
    pub filesystem: String,
    pub status: String,
    pub encrypted: bool,
    pub locked: bool,
    pub read_only: bool,
    pub full: bool,
    pub total_kb: u64,
    pub free_kb: u64,
    pub name: String,
    /// The parameter group of the disk (S0, S1), used by the param.cgi fallback.
    pub group: String,
    /// Days that unlocked recordings are kept (with the fifo policy); 0 is no age limit.
    /// None when the camera does not report it.
    pub max_age: Option<u32>,
    /// `fifo` (the camera removes old recordings) or `none` (only by hand). Empty when not reported.
    pub policy: String,
}

impl Disk {
    fn from_attrs(attrs: &Map<String, Value>) -> Self {
        let text = |k: &str| attrs.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let yes = |k: &str| matches!(text(k).as_str(), "yes" | "true");
        let num = |k: &str| text(k).parse::<u64>().unwrap_or(0);
        Self {
            id: text("diskid"),
            filesystem: text("filesystem"),
            status: text("status"),
            encrypted: yes("diskencrypted") || yes("encryptionenabled"),
            locked: yes("locked"),
            read_only: yes("readonly"),
            full: yes("full"),
            total_kb: num("totalsize"),
            free_kb: num("freesize"),
            name: text("name"),
            group: text("group"),
            max_age: text("cleanupmaxage").parse::<u32>().ok(),
            policy: text("cleanuppolicy").to_ascii_lowercase(),
        }
    }

    /// Retention as `disk status` and `disk retention` show it.
    pub fn retention(&self) -> Value {
        let removes = match (self.policy.as_str(), self.max_age) {
            ("none", _) => "never: recordings must be removed by hand, so the disk fills up".to_string(),
            ("fifo", Some(0)) => "the oldest recordings, when the disk is full".to_string(),
            ("fifo", Some(n)) => format!("recordings older than {n} days, and the oldest sooner when the disk is full"),
            _ => "unknown (the camera does not report it)".to_string(),
        };
        json!({
            "policy": if self.policy.is_empty() { Value::Null } else { json!(self.policy) },
            "maxAgeDays": self.max_age,
            "removes": removes,
        })
    }

    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.free_kb)
    }

    /// Network shares are managed elsewhere and cannot be formatted or encrypted here.
    pub fn is_local(&self) -> bool {
        let net = ["cifs", "smb", "nfs", "smbfs"];
        !(net.contains(&self.filesystem.to_ascii_lowercase().as_str()) || self.id.eq_ignore_ascii_case("networkshare"))
    }

    pub fn is_mounted(&self) -> bool {
        self.status.eq_ignore_ascii_case("ok")
    }
}

pub fn list(client: &VapixClient) -> anyhow::Result<Vec<Disk>> {
    Ok(storage::list_disks_xml(client)?.iter().map(Disk::from_attrs).collect())
}

/// The disk with this id, or an error that names the disks there are.
pub fn find(client: &VapixClient, id: &str) -> anyhow::Result<Disk> {
    let disks = list(client)?;
    match disks.iter().find(|d| d.id == id) {
        Some(d) => Ok(d.clone()),
        None => {
            let ids: Vec<&str> = disks.iter().map(|d| d.id.as_str()).collect();
            bail!("no disk '{}' on this camera (disks: {})", id, ids.join(", "))
        }
    }
}

/// A local disk that format and encryption may touch.
pub fn find_local(client: &VapixClient, id: &str) -> anyhow::Result<Disk> {
    let disk = find(client, id)?;
    if !disk.is_local() {
        bail!("'{}' is a network share, not a local disk; it cannot be formatted or encrypted here", disk.id);
    }
    Ok(disk)
}

fn kb_to_gb(kb: u64) -> f64 {
    (kb as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0
}

/// Recordings on a disk: how many, and the oldest and newest start time.
fn recordings_of(client: &VapixClient, disk: &str) -> anyhow::Result<Value> {
    let list = storage::list_recordings_filtered(client, Some(disk), None, None, MAX_COUNTED)?;
    let items = list.get("recordings").and_then(Value::as_array).cloned().unwrap_or_default();
    let reported = list.get("total").and_then(Value::as_u64).unwrap_or(0);
    let count = reported.max(items.len() as u64);
    let mut starts: Vec<&str> = items.iter().filter_map(|r| r.get("start").and_then(Value::as_str)).collect();
    starts.sort_unstable();
    Ok(json!({
        "count": count,
        "oldest": starts.first(),
        "newest": starts.last(),
        // The camera was not asked for more than MAX_COUNTED, so the dates then cover only part.
        "datesComplete": count <= items.len() as u64,
    }))
}

/// What a card holds, as the answer to `disk status`.
pub fn status(client: &VapixClient, only: Option<&str>) -> anyhow::Result<Value> {
    let disks = list(client)?;
    let health = storage::get_disk_health(client).ok();
    let mut out = Vec::new();
    for d in disks.iter().filter(|d| only.is_none_or(|id| d.id == id)) {
        let used = d.used_kb();
        let percent = if d.total_kb > 0 { (used as f64 / d.total_kb as f64 * 1000.0).round() / 10.0 } else { 0.0 };
        let recordings = if d.is_local() && d.is_mounted() {
            recordings_of(client, &d.id).unwrap_or_else(|e| json!({"error": e.to_string()}))
        } else {
            Value::Null
        };
        // Wear level and the like, when the camera has disks/properties.cgi.
        let details = health
            .as_ref()
            .and_then(|h| h.get("disks"))
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|e| e.get("id").and_then(Value::as_str) == Some(d.id.as_str())))
            .filter(|e| e.get("source").is_none())
            .cloned();
        out.push(json!({
            "id": d.id,
            "name": d.name,
            "local": d.is_local(),
            "filesystem": d.filesystem,
            "status": d.status,
            "encrypted": d.encrypted,
            "locked": d.locked,
            "readOnly": d.read_only,
            "full": d.full,
            "totalKb": d.total_kb,
            "freeKb": d.free_kb,
            "usedKb": used,
            "usedPercent": percent,
            "health": {"status": d.status, "details": details},
            "retention": d.retention(),
            "recordings": recordings,
        }));
    }
    if let Some(id) = only {
        if out.is_empty() {
            let ids: Vec<&str> = disks.iter().map(|d| d.id.as_str()).collect();
            bail!("no disk '{}' on this camera (disks: {})", id, ids.join(", "));
        }
    }
    Ok(json!({"disks": out}))
}

/// What an erase of this disk would remove: the numbers behind a dry run.
pub fn impact(client: &VapixClient, disk: &Disk) -> anyhow::Result<Value> {
    let recordings = if disk.is_mounted() {
        recordings_of(client, &disk.id)?
    } else {
        // An unmounted card cannot be asked; its contents are unknown, not zero.
        json!({"count": null, "oldest": null, "newest": null, "datesComplete": false})
    };
    Ok(json!({
        "disk": disk.id,
        "status": disk.status,
        "filesystem": disk.filesystem,
        "encrypted": disk.encrypted,
        "recordings": recordings["count"],
        "oldestRecording": recordings["oldest"],
        "newestRecording": recordings["newest"],
        "datesComplete": recordings["datesComplete"],
        "usedGb": kb_to_gb(disk.used_kb()),
        "totalGb": kb_to_gb(disk.total_kb),
    }))
}

// ---- retention ----

/// The cleanup policy of a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// The camera removes the oldest recordings: after the max age, and when the disk is full.
    Fifo,
    /// Recordings are only removed by hand.
    None,
}

impl Policy {
    pub fn as_str(self) -> &'static str {
        match self {
            Policy::Fifo => "fifo",
            Policy::None => "none",
        }
    }
}

/// A change of retention. Unset fields stay as they are.
#[derive(Debug, Clone, Copy, Default)]
pub struct Retention {
    pub max_age: Option<u32>,
    pub policy: Option<Policy>,
}

impl Retention {
    pub fn is_empty(&self) -> bool {
        self.max_age.is_none() && self.policy.is_none()
    }

    /// Refuse values the camera does not take, before anything is sent.
    pub fn check(&self) -> anyhow::Result<()> {
        if self.is_empty() {
            bail!("nothing to change: give a max age, a policy or both");
        }
        if let Some(days) = self.max_age {
            if days > MAX_AGE_LIMIT {
                bail!("max age {days} is out of range: 0 to {MAX_AGE_LIMIT} days (0 is no age limit)");
            }
        }
        Ok(())
    }

    /// Can this change make the camera remove recordings it keeps today? That is when the
    /// disk ends up with fifo and an age limit that is new or shorter than before.
    pub fn may_remove(&self, disk: &Disk) -> bool {
        let policy_after = self.policy.map(Policy::as_str).unwrap_or(disk.policy.as_str());
        let age_after = self.max_age.or(disk.max_age);
        if policy_after != "fifo" {
            return false;
        }
        let Some(after) = age_after.filter(|d| *d > 0) else { return false };
        let limit_before = if disk.policy == "fifo" { disk.max_age } else { None };
        match limit_before {
            Some(before) if before > 0 => after < before,
            // No limit before (0, policy none or not reported): any limit is new.
            _ => true,
        }
    }
}

/// The disks a retention command is about: one id, or every disk for `all`.
pub fn select(client: &VapixClient, id: &str) -> anyhow::Result<Vec<Disk>> {
    if id.eq_ignore_ascii_case("all") {
        let disks = list(client)?;
        if disks.is_empty() {
            bail!("this camera reports no disks");
        }
        Ok(disks)
    } else {
        Ok(vec![find(client, id)?])
    }
}

/// How a setting reached the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Via {
    DiskProperties,
    ParamCgi,
}

/// Set one cleanup property: the Disk properties API, or, when the camera does not have
/// it (404), the old `root.Storage.<group>.<key>` parameter.
fn set_cleanup(client: &VapixClient, disk: &Disk, path: &str, arg: &str, value: &str, legacy_key: &str) -> anyhow::Result<Via> {
    let query = [("schemaversion", "1"), ("diskid", disk.id.as_str()), (arg, value)];
    match client.get_text(path, &query) {
        Ok(body) => {
            check_error(&body)?;
            Ok(Via::DiskProperties)
        }
        Err(e) if e.to_string().contains("HTTP 404") => {
            debug!("{path} returned 404, using param.cgi for {}", disk.id);
            if disk.group.is_empty() {
                bail!("the camera has no {path} and does not report a parameter group for disk {}", disk.id);
            }
            let key = format!("root.Storage.{}.{}", disk.group, legacy_key);
            params::update(client, &[(key.as_str(), value)])?;
            Ok(Via::ParamCgi)
        }
        Err(e) => Err(e),
    }
}

/// Change the retention of one disk and read it back. The policy is set first, since the
/// max age only counts with fifo.
pub fn set_retention(client: &VapixClient, disk: &Disk, change: Retention) -> anyhow::Result<Value> {
    change.check()?;
    let before = disk.retention();
    let mut via = Vec::new();
    if let Some(policy) = change.policy {
        via.push(set_cleanup(client, disk, SET_POLICY, "cleanuppolicy", policy.as_str(), "CleanupPolicyActive")?);
    }
    if let Some(days) = change.max_age {
        let days = days.to_string();
        via.push(set_cleanup(client, disk, SET_MAX_AGE, "maxage", &days, "CleanupMaxAge")?);
    }
    let after = find(client, &disk.id)?;
    let mut checked = true;
    if let Some(days) = change.max_age {
        match after.max_age {
            Some(got) if got != days => bail!("disk {}: the camera reports max age {} days after setting {}", disk.id, got, days),
            None => checked = false,
            _ => {}
        }
    }
    if let Some(policy) = change.policy {
        if after.policy.is_empty() {
            checked = false;
        } else if after.policy != policy.as_str() {
            bail!("disk {}: the camera reports policy {} after setting {}", disk.id, after.policy, policy.as_str());
        }
    }
    let api = if via.contains(&Via::ParamCgi) { "param.cgi (Storage parameters)" } else { "disk properties API" };
    Ok(json!({
        "disk": disk.id,
        "before": before,
        "after": after.retention(),
        "via": api,
        "verified": checked,
    }))
}

// ---- jobs ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub state: JobState,
    pub progress: Option<u32>,
    pub message: String,
}

/// Fail on an error answer. The camera answers XML (`GeneralError` or an `error` element) or JSON.
fn check_error(body: &str) -> anyhow::Result<()> {
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(err) = v.get("error") {
                let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown error");
                bail!("the camera refused: {}", msg);
            }
        }
        return Ok(());
    }
    let Ok(doc) = roxmltree::Document::parse(trimmed) else { return Ok(()) };
    for n in doc.descendants().filter(|n| n.is_element()) {
        let name = n.tag_name().name().to_ascii_lowercase();
        if name == "generalerror" || name == "error" {
            let text = n
                .descendants()
                .find(|c| c.tag_name().name().eq_ignore_ascii_case("ErrorDescription"))
                .and_then(|c| c.text())
                .or_else(|| n.attribute("description"))
                .or_else(|| n.attribute("message"))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    let attrs: Vec<String> = n.attributes().map(|a| format!("{}={}", a.name(), a.value())).collect();
                    if attrs.is_empty() { "unknown error".into() } else { attrs.join(" ") }
                });
            bail!("the camera refused: {}", text);
        }
    }
    Ok(())
}

/// The job in an answer, if there is one. Looks for an element with a `jobid`.
pub fn parse_job(body: &str) -> anyhow::Result<Option<Job>> {
    check_error(body)?;
    let Ok(doc) = roxmltree::Document::parse(body.trim()) else { return Ok(None) };
    let Some(node) = doc.descendants().find(|n| n.is_element() && n.attribute("jobid").is_some()) else {
        return Ok(None);
    };
    let field = |k: &str| {
        node.attribute(k)
            .map(str::to_string)
            .or_else(|| node.children().find(|c| c.tag_name().name().eq_ignore_ascii_case(k)).and_then(|c| c.text()).map(|t| t.trim().to_string()))
            .unwrap_or_default()
    };
    let status = field("status").to_ascii_lowercase();
    let result = field("result").to_ascii_lowercase();
    let progress = field("progress").trim_end_matches('%').parse::<u32>().ok();
    let failed = |s: &str| s.contains("fail") || s.contains("error");
    let done = |s: &str| matches!(s, "completed" | "complete" | "finished" | "done" | "ok" | "success" | "successful");
    let state = if failed(&status) || failed(&result) {
        JobState::Failed
    } else if done(&status) || done(&result) || progress.is_some_and(|p| p >= 100) {
        JobState::Done
    } else {
        JobState::Running
    };
    let message = [field("description"), field("errordescription")].into_iter().find(|s| !s.is_empty()).unwrap_or_default();
    Ok(Some(Job { id: node.attribute("jobid").unwrap_or("").to_string(), state, progress, message }))
}

/// How long to wait for a job and how often to ask.
#[derive(Debug, Clone, Copy)]
pub struct Wait {
    pub interval: Duration,
    pub timeout: Duration,
}

impl Default for Wait {
    fn default() -> Self {
        Self { interval: Duration::from_secs(2), timeout: Duration::from_secs(30 * 60) }
    }
}

/// Ask for the job until it is done. `on_progress` sees every answer.
pub fn follow(
    mut fetch: impl FnMut() -> anyhow::Result<Job>,
    wait: Wait,
    mut on_progress: impl FnMut(&Job),
) -> anyhow::Result<Job> {
    let started = Instant::now();
    loop {
        let job = fetch()?;
        on_progress(&job);
        match job.state {
            JobState::Done => return Ok(job),
            JobState::Failed => {
                let why = if job.message.is_empty() { "no reason given".to_string() } else { job.message.clone() };
                bail!("job {} failed: {}", job.id, why)
            }
            JobState::Running => {}
        }
        if started.elapsed() >= wait.timeout {
            bail!(
                "job {} still running after {} s; the camera goes on with it, check `vapx disk status`",
                job.id,
                wait.timeout.as_secs()
            );
        }
        std::thread::sleep(wait.interval);
    }
}

fn follow_job(client: &VapixClient, disk: &str, job: Job, wait: Wait, on_progress: impl FnMut(&Job)) -> anyhow::Result<Job> {
    if job.state == JobState::Done {
        return Ok(job);
    }
    let id = job.id.clone();
    follow(
        || {
            let body = client.get_text(JOB, &[("schemaversion", "1"), ("jobid", &id), ("diskid", disk)])?;
            parse_job(&body)?.ok_or_else(|| anyhow::anyhow!("the camera did not report job {}", id))
        },
        wait,
        on_progress,
    )
}

// ---- calls ----

/// Call a disk CGI with GET and follow the job it returns, if any.
fn run_get(
    client: &VapixClient,
    path: &str,
    disk: &str,
    extra: &[(&str, &str)],
    wait: Wait,
    on_progress: impl FnMut(&Job),
) -> anyhow::Result<Option<Job>> {
    let mut query = vec![("schemaversion", "1"), ("diskid", disk)];
    query.extend_from_slice(extra);
    let body = client.get_text(path, &query)?;
    match parse_job(&body)? {
        Some(job) => Ok(Some(follow_job(client, disk, job, wait, on_progress)?)),
        None => Ok(None),
    }
}

/// Call a disk CGI that takes secrets: a POST form, never a URL.
fn run_post(
    client: &VapixClient,
    path: &str,
    disk: &str,
    fields: &[(&str, &str)],
    wait: Wait,
    on_progress: impl FnMut(&Job),
) -> anyhow::Result<Option<Job>> {
    let mut form = vec![("schemaversion", "1"), ("diskid", disk)];
    form.extend_from_slice(fields);
    let body = client.post_form_text(path, &form)?;
    match parse_job(&body)? {
        Some(job) => Ok(Some(follow_job(client, disk, job, wait, on_progress)?)),
        None => Ok(None),
    }
}

pub fn mount(client: &VapixClient, disk: &str, wait: Wait, on_progress: impl FnMut(&Job)) -> anyhow::Result<()> {
    run_get(client, MOUNT, disk, &[], wait, on_progress).map(|_| ())
}

pub fn unmount(client: &VapixClient, disk: &str, wait: Wait, on_progress: impl FnMut(&Job)) -> anyhow::Result<()> {
    run_get(client, UNMOUNT, disk, &[], wait, on_progress).map(|_| ())
}

/// Wait until the disk reports status OK again (after a mount).
fn wait_until_mounted(client: &VapixClient, id: &str, wait: Wait) -> anyhow::Result<Disk> {
    let started = Instant::now();
    loop {
        let disk = find(client, id)?;
        if disk.is_mounted() {
            return Ok(disk);
        }
        if started.elapsed() >= wait.timeout.min(Duration::from_secs(120)) {
            bail!("disk {} is {} after the operation, not OK; check `vapx disk status`", id, disk.status);
        }
        std::thread::sleep(wait.interval);
    }
}

fn unmount_if_mounted(client: &VapixClient, disk: &Disk, wait: Wait, mut on_progress: impl FnMut(&Job)) -> anyhow::Result<()> {
    if disk.is_mounted() {
        debug!("unmounting {}", disk.id);
        unmount(client, &disk.id, wait, &mut on_progress)?;
    }
    Ok(())
}

/// Format a local disk: unmount, format, follow the job, mount. Erases every recording.
pub fn format(
    client: &VapixClient,
    id: &str,
    filesystem: &str,
    wait: Wait,
    mut on_progress: impl FnMut(&str, &Job),
) -> anyhow::Result<Value> {
    if !matches!(filesystem, "ext4" | "xfs" | "vfat") {
        bail!("filesystem must be ext4, xfs or vfat");
    }
    let disk = find_local(client, id)?;
    unmount_if_mounted(client, &disk, wait, |j| on_progress("unmount", j))?;
    run_get(client, FORMAT, id, &[("filesystem", filesystem)], wait, |j| on_progress("format", j))?;
    mount(client, id, wait, |j| on_progress("mount", j))?;
    let after = wait_until_mounted(client, id, wait)?;
    Ok(json!({
        "disk": after.id,
        "formatted": true,
        "filesystem": after.filesystem,
        "status": after.status,
        "encrypted": after.encrypted,
    }))
}

/// Does this camera have disk encryption? (`Properties.LocalStorage.DiskEncryption`.)
pub fn supports_encryption(client: &VapixClient) -> anyhow::Result<bool> {
    let text = params::list(client, Some("root.Properties.LocalStorage.DiskEncryption"))?;
    Ok(text.lines().any(|l| l.trim().ends_with("DiskEncryption=yes")))
}

/// Switch encryption on or off: unmount, switch (the camera formats the card), follow the job,
/// mount, and check that the card reports the new state. Erases every recording.
pub fn set_encryption(
    client: &VapixClient,
    id: &str,
    enable: bool,
    passphrase: &Secret,
    wait: Wait,
    mut on_progress: impl FnMut(&str, &Job),
) -> anyhow::Result<Value> {
    if !supports_encryption(client)? {
        bail!("this camera has no disk encryption (Properties.LocalStorage.DiskEncryption is not yes)");
    }
    let disk = find_local(client, id)?;
    if disk.encrypted == enable {
        bail!("disk {} is already {}; nothing to do", id, if enable { "encrypted" } else { "unencrypted" });
    }
    unmount_if_mounted(client, &disk, wait, |j| on_progress("unmount", j))?;
    let path = if enable { ENABLE_ENCRYPTION } else { DISABLE_ENCRYPTION };
    run_post(client, path, id, &[("passphrase", passphrase.expose())], wait, |j| on_progress("encryption", j))?;
    // A card that is not mounted by the camera itself afterwards is mounted here. A card that is
    // locked (encrypted, passphrase not given) stays unmounted, which is reported below.
    let after_switch = find(client, id)?;
    if !after_switch.is_mounted() {
        mount(client, id, wait, |j| on_progress("mount", j))?;
    }
    let after = wait_until_mounted(client, id, wait)?;
    if after.encrypted != enable {
        bail!(
            "the camera reports disk {} as {} after the switch; check `vapx disk status`",
            id,
            if after.encrypted { "encrypted" } else { "unencrypted" }
        );
    }
    Ok(json!({
        "disk": after.id,
        "encrypted": after.encrypted,
        "filesystem": after.filesystem,
        "status": after.status,
    }))
}

/// Change the passphrase of an encrypted disk. The recordings stay.
pub fn change_passphrase(client: &VapixClient, id: &str, current: &Secret, new: &Secret, wait: Wait) -> anyhow::Result<Value> {
    if !supports_encryption(client)? {
        bail!("this camera has no disk encryption (Properties.LocalStorage.DiskEncryption is not yes)");
    }
    let disk = find_local(client, id)?;
    if !disk.encrypted {
        bail!("disk {} is not encrypted; there is no passphrase to change", id);
    }
    run_post(
        client,
        CHANGE_PASSPHRASE,
        id,
        &[("passphrase", current.expose()), ("newpassphrase", new.expose())],
        wait,
        |_| {},
    )?;
    Ok(json!({"disk": id, "passphraseChanged": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOB_RUNNING: &str = r#"<?xml version="1.0"?><root><job jobid="7" diskid="SD_DISK" status="ongoing" progress="40"/></root>"#;
    const JOB_DONE: &str = r#"<root><job jobid="7" diskid="SD_DISK" status="completed" progress="100"/></root>"#;
    const JOB_FAILED: &str = r#"<root><job jobid="7" diskid="SD_DISK" status="failed" description="disk busy"/></root>"#;

    fn job(state: JobState) -> Job {
        Job { id: "7".into(), state, progress: None, message: String::new() }
    }

    #[test]
    fn parses_running_done_and_failed_jobs() {
        let running = parse_job(JOB_RUNNING).unwrap().unwrap();
        assert_eq!((running.id.as_str(), running.state, running.progress), ("7", JobState::Running, Some(40)));
        assert_eq!(parse_job(JOB_DONE).unwrap().unwrap().state, JobState::Done);
        let failed = parse_job(JOB_FAILED).unwrap().unwrap();
        assert_eq!((failed.state, failed.message.as_str()), (JobState::Failed, "disk busy"));
    }

    #[test]
    fn an_answer_without_a_job_is_not_a_job() {
        assert!(parse_job("<root/>").unwrap().is_none());
        assert!(parse_job("").unwrap().is_none());
    }

    #[test]
    fn error_answers_become_errors() {
        let xml = "<root><GeneralError><ErrorCode>1</ErrorCode><ErrorDescription>Disk is mounted</ErrorDescription></GeneralError></root>";
        assert!(parse_job(xml).unwrap_err().to_string().contains("Disk is mounted"));
        let json = r#"{"error":{"code":1000,"message":"bad passphrase"}}"#;
        assert!(parse_job(json).unwrap_err().to_string().contains("bad passphrase"));
    }

    #[test]
    fn follow_asks_until_the_job_is_done_and_reports_each_answer() {
        let mut answers = vec![JobState::Running, JobState::Running, JobState::Done].into_iter();
        let mut seen = 0;
        let wait = Wait { interval: Duration::ZERO, timeout: Duration::from_secs(5) };
        let done = follow(|| Ok(job(answers.next().unwrap())), wait, |_| seen += 1).unwrap();
        assert_eq!(done.state, JobState::Done);
        assert_eq!(seen, 3);
    }

    #[test]
    fn follow_stops_with_the_reason_when_the_job_fails() {
        let wait = Wait { interval: Duration::ZERO, timeout: Duration::from_secs(5) };
        let err = follow(
            || Ok(Job { message: "no space".into(), ..job(JobState::Failed) }),
            wait,
            |_| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("no space"), "{err}");
    }

    #[test]
    fn follow_gives_up_at_the_timeout_and_says_the_camera_goes_on() {
        let wait = Wait { interval: Duration::ZERO, timeout: Duration::ZERO };
        let err = follow(|| Ok(job(JobState::Running)), wait, |_| {}).unwrap_err().to_string();
        assert!(err.contains("still running") && err.contains("goes on"), "{err}");
    }

    fn disk_with(policy: &str, max_age: Option<u32>) -> Disk {
        let mut attrs: Map<String, Value> = serde_json::from_value(json!({
            "diskid": "SD_DISK", "group": "S0", "filesystem": "ext4", "status": "OK", "cleanuppolicy": policy
        }))
        .unwrap();
        if let Some(d) = max_age {
            attrs.insert("cleanupmaxage".into(), json!(d.to_string()));
        }
        Disk::from_attrs(&attrs)
    }

    #[test]
    fn retention_is_read_from_list_attributes() {
        let d = disk_with("fifo", Some(30));
        assert_eq!((d.group.as_str(), d.max_age, d.policy.as_str()), ("S0", Some(30), "fifo"));
        assert_eq!(d.retention()["maxAgeDays"], 30);
        assert!(d.retention()["removes"].as_str().unwrap().contains("older than 30 days"));
        assert!(disk_with("fifo", Some(0)).retention()["removes"].as_str().unwrap().contains("when the disk is full"));
        assert!(disk_with("none", Some(7)).retention()["removes"].as_str().unwrap().starts_with("never"));
        assert_eq!(disk_with("", None).retention()["policy"], Value::Null);
    }

    #[test]
    fn a_retention_change_must_change_something_and_stay_in_range() {
        assert!(Retention::default().check().is_err());
        assert!(Retention { max_age: Some(MAX_AGE_LIMIT + 1), policy: None }.check().is_err());
        assert!(Retention { max_age: Some(MAX_AGE_LIMIT), policy: None }.check().is_ok());
        assert!(Retention { max_age: Some(0), policy: None }.check().is_ok());
        assert!(Retention { max_age: None, policy: Some(Policy::None) }.check().is_ok());
    }

    #[test]
    fn only_a_new_or_shorter_age_limit_may_remove_recordings() {
        let age = |d: u32| Retention { max_age: Some(d), policy: None };
        // Longer, or no limit at all: nothing goes earlier than before.
        assert!(!age(60).may_remove(&disk_with("fifo", Some(30))));
        assert!(!age(0).may_remove(&disk_with("fifo", Some(30))));
        assert!(!age(30).may_remove(&disk_with("fifo", Some(30))));
        // Shorter, or a limit where there was none.
        assert!(age(7).may_remove(&disk_with("fifo", Some(30))));
        assert!(age(7).may_remove(&disk_with("fifo", Some(0))));
        // From none to fifo turns the old limit on.
        assert!(Retention { max_age: None, policy: Some(Policy::Fifo) }.may_remove(&disk_with("none", Some(7))));
        assert!(!Retention { max_age: None, policy: Some(Policy::Fifo) }.may_remove(&disk_with("none", Some(0))));
        // Policy none removes nothing.
        assert!(!Retention { max_age: Some(1), policy: Some(Policy::None) }.may_remove(&disk_with("fifo", Some(0))));
    }

    #[test]
    fn disks_are_read_from_list_attributes() {
        let attrs: Map<String, Value> = serde_json::from_value(json!({
            "diskid": "SD_DISK", "filesystem": "ext4", "status": "OK", "diskencrypted": "true",
            "locked": "no", "readonly": "no", "full": "no", "totalsize": "2097152", "freesize": "1048576"
        }))
        .unwrap();
        let d = Disk::from_attrs(&attrs);
        assert!(d.encrypted && d.is_local() && d.is_mounted() && !d.locked);
        assert_eq!(d.used_kb(), 1_048_576);
        assert_eq!(kb_to_gb(d.used_kb()), 1.0);
    }

    #[test]
    fn a_network_share_is_not_a_local_disk() {
        let attrs: Map<String, Value> =
            serde_json::from_value(json!({"diskid": "NetworkShare", "filesystem": "cifs", "status": "disconnected"})).unwrap();
        assert!(!Disk::from_attrs(&attrs).is_local());
    }

    #[test]
    fn a_secret_never_shows_in_debug_output_and_must_not_be_empty() {
        let s = Secret::new("hunter2-passphrase".into()).unwrap();
        assert!(!format!("{s:?}").contains("hunter2"));
        assert!(Secret::new(String::new()).is_err());
    }

    #[test]
    fn the_passphrase_comes_from_a_private_file_or_the_environment() {
        let dir = std::env::temp_dir().join(format!("vapx-disk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pass");
        std::fs::write(&file, "correct horse\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            let err = secret_from(Some(&file), "VAPX_TEST_UNSET", "passphrase").unwrap_err().to_string();
            assert!(err.contains("readable by others") && !err.contains("correct horse"), "{err}");
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let s = secret_from(Some(&file), "VAPX_TEST_UNSET", "passphrase").unwrap();
        assert_eq!(s.expose(), "correct horse");
        let err = secret_from(None, "VAPX_TEST_DEFINITELY_UNSET", "passphrase").unwrap_err().to_string();
        assert!(err.contains("VAPX_TEST_DEFINITELY_UNSET"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
