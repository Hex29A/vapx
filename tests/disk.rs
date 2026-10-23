//! `vapx disk` against a camera stand-in (#82): what each command sends, in which order, and
//! what it refuses to do. They need no camera. The passphrase must never show up in a URL, in
//! the output or on the command line.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

type Log = Arc<Mutex<Vec<String>>>;

const PASSPHRASE: &str = "correct-horse-battery-staple";

/// One path and the bodies it answers with, in order; the last one repeats.
struct Route(&'static str, Vec<String>);

fn route(path: &'static str, bodies: &[&str]) -> Route {
    Route(path, bodies.iter().map(|b| b.to_string()).collect())
}

/// A camera stand-in: answers by path (no authentication challenge) and records `METHOD target body`.
fn stub(routes: Vec<Route>) -> (u16, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Log = Default::default();
    let shared = log.clone();
    let routes = Arc::new(Mutex::new(routes));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (routes, log) = (routes.clone(), shared.clone());
            thread::spawn(move || {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let head_end = loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                let length = head
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                    .unwrap_or(0);
                while data.len() < head_end + length {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                let body = String::from_utf8_lossy(&data[head_end..]).into_owned();
                let mut first = head.lines().next().unwrap_or("").split_whitespace();
                let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or("/"));
                log.lock().unwrap().push(format!("{method} {target} {body}"));
                let path = target.split('?').next().unwrap();
                let reply = {
                    let mut routes = routes.lock().unwrap();
                    routes.iter_mut().find(|r| r.0 == path).map(|r| if r.1.len() > 1 { r.1.remove(0) } else { r.1[0].clone() })
                };
                let Some(reply) = reply else {
                    let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    return;
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            });
        }
    });
    (port, log)
}

fn config(tag: &str, port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-disk-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    std::fs::write(&path, format!("cameras:\n  cam1:\n    host: 127.0.0.1\n    port: {port}\n    user: root\n    pass: pass\n")).unwrap();
    path
}

struct Run {
    ok: bool,
    out: String,
    err: String,
    reqs: Vec<String>,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.out))
    }

    /// The requests whose "METHOD path" starts with this prefix.
    fn hits(&self, prefix: &str) -> Vec<&String> {
        self.reqs.iter().filter(|r| r.starts_with(prefix)).collect()
    }

    /// Where the first request with this prefix sits in the order of requests.
    fn at(&self, prefix: &str) -> usize {
        self.reqs.iter().position(|r| r.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix}: {:?}", self.reqs))
    }
}

fn run(tag: &str, routes: Vec<Route>, env: &[(&str, &str)], args: &[&str]) -> Run {
    let (port, log) = stub(routes);
    let cfg = config(tag, port);
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .arg("--config")
        .arg(&cfg)
        .arg("disk")
        .args(args)
        .arg("cam1")
        .env_remove("VAPX_DISK_PASSPHRASE")
        .env_remove("VAPX_DISK_NEW_PASSPHRASE")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let reqs = log.lock().unwrap().clone();
    Run {
        ok: out.status.success(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
        reqs,
    }
}

fn disk_xml(id: &str, fs: &str, status: &str, encrypted: bool) -> String {
    format!(
        r#"<root><disks><disk diskid="{id}" name="" group="S0" filesystem="{fs}" status="{status}" diskencrypted="{encrypted}" encryptionenabled="{encrypted}" locked="no" readonly="no" full="no" totalsize="2097152" freesize="1048576"/></disks></root>"#
    )
}

const RECORDINGS: &str = r#"<root><recordings totalnumberofrecordings="2">
<recording recordingid="b" diskid="SD_DISK" starttime="2026-10-02T08:00:00Z" stoptime="2026-10-02T09:00:00Z"/>
<recording recordingid="a" diskid="SD_DISK" starttime="2026-10-01T10:00:00Z" stoptime="2026-10-01T11:00:00Z"/>
</recordings></root>"#;
const NO_RECORDINGS: &str = r#"<root><recordings totalnumberofrecordings="0"></recordings></root>"#;
const ENCRYPTION_YES: &str = "root.Properties.LocalStorage.DiskEncryption=yes\n";
const JOB_STARTED: &str = r#"<root><job jobid="7" diskid="SD_DISK" status="ongoing" progress="0"/></root>"#;
const JOB_DONE: &str = r#"<root><job jobid="7" diskid="SD_DISK" status="completed" progress="100"/></root>"#;
const JOB_FAILED: &str = r#"<root><job jobid="7" diskid="SD_DISK" status="failed" description="disk busy"/></root>"#;
const OK: &str = "<root/>";

/// The request target (path and query) of a logged `METHOD target body` line.
fn target_of(req: &str) -> &str {
    req.split_whitespace().nth(1).unwrap_or("")
}

fn assert_no_passphrase(r: &Run) {
    assert!(!r.out.contains(PASSPHRASE) && !r.err.contains(PASSPHRASE), "passphrase in output: {} {}", r.out, r.err);
    for req in &r.reqs {
        let target = target_of(req);
        assert!(!target.contains(PASSPHRASE), "passphrase in a URL: {target}");
    }
}

// ---- status ----

#[test]
fn status_shows_encryption_file_system_usage_and_recordings() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", true);
    let r = run(
        "status",
        vec![route("/axis-cgi/disks/list.cgi", &[&ok]), route("/axis-cgi/record/list.cgi", &[RECORDINGS])],
        &[],
        &["status"],
    );
    assert!(r.ok, "{}", r.err);
    let d = &r.json()["data"]["disks"][0];
    assert_eq!(d["id"], "SD_DISK");
    assert_eq!(d["encrypted"], true);
    assert_eq!(d["filesystem"], "ext4");
    assert_eq!(d["status"], "OK");
    assert_eq!(d["usedPercent"], 50.0);
    assert_eq!(d["recordings"]["count"], 2);
    assert_eq!(d["recordings"]["oldest"], "2026-10-01T10:00:00Z");
}

#[test]
fn status_of_a_disk_that_does_not_exist_names_the_disks_there_are() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run("status-missing", vec![route("/axis-cgi/disks/list.cgi", &[&ok])], &[], &["status", "--disk", "NOPE"]);
    assert!(!r.ok);
    assert!(r.err.contains("SD_DISK"), "{}", r.err);
}

// ---- format ----

#[test]
fn format_without_yes_only_shows_what_would_be_erased() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "format-dry",
        vec![route("/axis-cgi/disks/list.cgi", &[&ok]), route("/axis-cgi/record/list.cgi", &[RECORDINGS])],
        &[],
        &["format"],
    );
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    let erase = &v["data"]["wouldErase"];
    assert_eq!(v["data"]["performed"], false);
    assert_eq!(erase["recordings"], 2);
    assert_eq!(erase["oldestRecording"], "2026-10-01T10:00:00Z");
    assert_eq!(erase["usedGb"], 1.0);
    for forbidden in ["format.cgi", "unmount.cgi", "mount.cgi"] {
        assert!(r.hits(&format!("GET /axis-cgi/disks/{forbidden}")).is_empty(), "{:?}", r.reqs);
    }
}

#[test]
fn format_with_yes_unmounts_formats_follows_the_job_and_mounts() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "format-yes",
        vec![
            route("/axis-cgi/disks/list.cgi", &[&ok]),
            route("/axis-cgi/disks/unmount.cgi", &[OK]),
            route("/axis-cgi/disks/format.cgi", &[JOB_STARTED]),
            route("/axis-cgi/disks/job.cgi", &[JOB_STARTED, JOB_DONE]),
            route("/axis-cgi/disks/mount.cgi", &[OK]),
        ],
        &[],
        &["format", "--yes", "--poll-interval", "0"],
    );
    assert!(r.ok, "{}", r.err);
    let (unmount, format, job, mount) = (
        r.at("GET /axis-cgi/disks/unmount.cgi"),
        r.at("GET /axis-cgi/disks/format.cgi"),
        r.at("GET /axis-cgi/disks/job.cgi"),
        r.at("GET /axis-cgi/disks/mount.cgi"),
    );
    assert!(unmount < format && format < job && job < mount, "{:?}", r.reqs);
    assert!(r.reqs[format].contains("diskid=SD_DISK") && r.reqs[format].contains("filesystem=ext4"), "{}", r.reqs[format]);
    assert_eq!(r.hits("GET /axis-cgi/disks/job.cgi").len(), 2, "the job is asked for until it is done");
    assert_eq!(r.json()["data"]["performed"], true);
}

#[test]
fn a_failed_format_job_stops_with_the_reason_and_does_not_mount() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "format-fail",
        vec![
            route("/axis-cgi/disks/list.cgi", &[&ok]),
            route("/axis-cgi/disks/unmount.cgi", &[OK]),
            route("/axis-cgi/disks/format.cgi", &[JOB_STARTED]),
            route("/axis-cgi/disks/job.cgi", &[JOB_FAILED]),
        ],
        &[],
        &["format", "--yes", "--poll-interval", "0"],
    );
    assert!(!r.ok);
    assert!(r.err.contains("disk busy"), "{}", r.err);
    assert!(r.hits("GET /axis-cgi/disks/mount.cgi").is_empty(), "{:?}", r.reqs);
}

#[test]
fn a_network_share_is_never_formatted() {
    let share = disk_xml("NetworkShare", "cifs", "OK", false);
    let r = run("format-share", vec![route("/axis-cgi/disks/list.cgi", &[&share])], &[], &["format", "--yes", "--disk", "NetworkShare"]);
    assert!(!r.ok);
    assert!(r.err.contains("network share"), "{}", r.err);
    assert!(r.hits("GET /axis-cgi/disks/format.cgi").is_empty(), "{:?}", r.reqs);
}

#[test]
fn an_unknown_file_system_is_refused_before_anything_is_sent() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "format-fs",
        vec![route("/axis-cgi/disks/list.cgi", &[&ok]), route("/axis-cgi/record/list.cgi", &[NO_RECORDINGS])],
        &[],
        &["format", "--yes", "--filesystem", "ntfs"],
    );
    assert!(!r.ok);
    assert!(r.hits("GET /axis-cgi/disks/unmount.cgi").is_empty() && r.hits("GET /axis-cgi/disks/format.cgi").is_empty(), "{:?}", r.reqs);
}

// ---- encryption ----

#[test]
fn encrypt_dry_run_shows_the_loss_and_whether_a_passphrase_is_available() {
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let routes = || {
        vec![
            route("/axis-cgi/disks/list.cgi", &[&plain]),
            route("/axis-cgi/record/list.cgi", &[RECORDINGS]),
            route("/axis-cgi/param.cgi", &[ENCRYPTION_YES]),
        ]
    };
    let with = run("enc-dry", routes(), &[("VAPX_DISK_PASSPHRASE", PASSPHRASE)], &["encrypt", "on"]);
    assert!(with.ok, "{}", with.err);
    assert_eq!(with.json()["data"]["passphraseAvailable"], true);
    assert_eq!(with.json()["data"]["wouldErase"]["recordings"], 2);
    assert_no_passphrase(&with);
    assert!(with.hits("POST").is_empty(), "{:?}", with.reqs);

    let without = run("enc-dry-no", routes(), &[], &["encrypt", "on"]);
    assert!(without.ok, "{}", without.err);
    assert_eq!(without.json()["data"]["passphraseAvailable"], false);
}

#[test]
fn encrypt_on_sends_the_passphrase_in_the_body_only_and_checks_the_result() {
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let locked_out = disk_xml("SD_DISK", "ext4", "disconnected", true);
    let done = disk_xml("SD_DISK", "ext4", "OK", true);
    let r = run(
        "enc-on",
        vec![
            // the command's check, the library's check, after the switch (card not mounted yet), after the mount
            route("/axis-cgi/disks/list.cgi", &[&plain, &plain, &locked_out, &done]),
            route("/axis-cgi/param.cgi", &[ENCRYPTION_YES]),
            route("/axis-cgi/disks/unmount.cgi", &[OK]),
            route("/axis-cgi/disks/enablediskencryption.cgi", &[JOB_STARTED]),
            route("/axis-cgi/disks/job.cgi", &[JOB_DONE]),
            route("/axis-cgi/disks/mount.cgi", &[OK]),
        ],
        &[("VAPX_DISK_PASSPHRASE", PASSPHRASE)],
        &["encrypt", "on", "--yes", "--poll-interval", "0"],
    );
    assert!(r.ok, "{}", r.err);
    let post = r.hits("POST /axis-cgi/disks/enablediskencryption.cgi");
    assert_eq!(post.len(), 1, "{:?}", r.reqs);
    assert!(post[0].contains(&format!("passphrase={PASSPHRASE}")), "the body carries the passphrase: {}", post[0]);
    assert_eq!(target_of(post[0]), "/axis-cgi/disks/enablediskencryption.cgi", "no query string");
    assert!(r.at("GET /axis-cgi/disks/unmount.cgi") < r.at("POST /axis-cgi/disks/enablediskencryption.cgi"));
    assert!(r.at("POST /axis-cgi/disks/enablediskencryption.cgi") < r.at("GET /axis-cgi/disks/mount.cgi"));
    assert_eq!(r.json()["data"]["encrypted"], true);
    assert_no_passphrase(&r);
}

#[test]
fn encrypt_off_uses_the_disable_call() {
    let enc = disk_xml("SD_DISK", "ext4", "OK", true);
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "enc-off",
        vec![
            route("/axis-cgi/disks/list.cgi", &[&enc, &enc, &plain]),
            route("/axis-cgi/param.cgi", &[ENCRYPTION_YES]),
            route("/axis-cgi/disks/unmount.cgi", &[OK]),
            route("/axis-cgi/disks/disablediskencryption.cgi", &[OK]),
            route("/axis-cgi/disks/mount.cgi", &[OK]),
        ],
        &[("VAPX_DISK_PASSPHRASE", PASSPHRASE)],
        &["encrypt", "off", "--yes", "--poll-interval", "0"],
    );
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.hits("POST /axis-cgi/disks/disablediskencryption.cgi").len(), 1, "{:?}", r.reqs);
    assert_eq!(r.json()["data"]["encrypted"], false);
    assert_no_passphrase(&r);
}

#[test]
fn encrypt_stops_when_the_disk_already_is_in_the_wanted_state() {
    let enc = disk_xml("SD_DISK", "ext4", "OK", true);
    let r = run(
        "enc-already",
        vec![route("/axis-cgi/disks/list.cgi", &[&enc]), route("/axis-cgi/param.cgi", &[ENCRYPTION_YES])],
        &[("VAPX_DISK_PASSPHRASE", PASSPHRASE)],
        &["encrypt", "on", "--yes"],
    );
    assert!(!r.ok);
    assert!(r.err.contains("already encrypted"), "{}", r.err);
    assert!(r.hits("POST").is_empty() && r.hits("GET /axis-cgi/disks/unmount.cgi").is_empty(), "{:?}", r.reqs);
}

#[test]
fn encrypt_on_a_camera_without_disk_encryption_does_nothing() {
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "enc-unsupported",
        vec![route("/axis-cgi/disks/list.cgi", &[&plain]), route("/axis-cgi/param.cgi", &["root.Properties.LocalStorage.DiskEncryption=no\n"])],
        &[("VAPX_DISK_PASSPHRASE", PASSPHRASE)],
        &["encrypt", "on", "--yes"],
    );
    assert!(!r.ok);
    assert!(r.err.contains("no disk encryption"), "{}", r.err);
    assert!(r.hits("POST").is_empty(), "{:?}", r.reqs);
}

#[test]
fn encrypt_with_yes_but_no_passphrase_changes_nothing() {
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let r = run(
        "enc-nopass",
        vec![route("/axis-cgi/disks/list.cgi", &[&plain]), route("/axis-cgi/param.cgi", &[ENCRYPTION_YES])],
        &[],
        &["encrypt", "on", "--yes"],
    );
    assert!(!r.ok);
    assert!(r.err.contains("VAPX_DISK_PASSPHRASE"), "{}", r.err);
    assert!(r.hits("POST").is_empty() && r.hits("GET /axis-cgi/disks/unmount.cgi").is_empty(), "{:?}", r.reqs);
}

#[cfg(unix)]
#[test]
fn a_passphrase_file_that_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let plain = disk_xml("SD_DISK", "ext4", "OK", false);
    let dir = std::env::temp_dir().join(format!("vapx-disk-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pass");
    std::fs::write(&file, format!("{PASSPHRASE}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let r = run(
        "enc-file",
        vec![route("/axis-cgi/disks/list.cgi", &[&plain]), route("/axis-cgi/param.cgi", &[ENCRYPTION_YES])],
        &[],
        &["encrypt", "on", "--yes", "--passphrase-file", file.to_str().unwrap()],
    );
    assert!(!r.ok);
    assert!(r.err.contains("readable by others"), "{}", r.err);
    assert_no_passphrase(&r);
    assert!(r.hits("POST").is_empty(), "{:?}", r.reqs);
}

#[test]
fn changing_the_passphrase_sends_both_in_the_body_and_keeps_the_recordings() {
    let enc = disk_xml("SD_DISK", "ext4", "OK", true);
    let new = "a-new-and-different-passphrase";
    let r = run(
        "pass-change",
        vec![
            route("/axis-cgi/disks/list.cgi", &[&enc]),
            route("/axis-cgi/param.cgi", &[ENCRYPTION_YES]),
            route("/axis-cgi/disks/changediskpassphrase.cgi", &[OK]),
        ],
        &[("VAPX_DISK_PASSPHRASE", PASSPHRASE), ("VAPX_DISK_NEW_PASSPHRASE", new)],
        &["passphrase"],
    );
    assert!(r.ok, "{}", r.err);
    let post = r.hits("POST /axis-cgi/disks/changediskpassphrase.cgi");
    assert_eq!(post.len(), 1, "{:?}", r.reqs);
    assert!(post[0].contains(&format!("passphrase={PASSPHRASE}")) && post[0].contains(&format!("newpassphrase={new}")), "{}", post[0]);
    assert!(r.hits("GET /axis-cgi/disks/unmount.cgi").is_empty() && r.hits("GET /axis-cgi/disks/format.cgi").is_empty(), "{:?}", r.reqs);
    assert!(!r.out.contains(new) && !r.err.contains(new));
    assert_no_passphrase(&r);
}

// ---- mount ----

#[test]
fn mount_and_unmount_are_separate_calls() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    for (cmd, path) in [("mount", "/axis-cgi/disks/mount.cgi"), ("unmount", "/axis-cgi/disks/unmount.cgi")] {
        let r = run(
            cmd,
            vec![route("/axis-cgi/disks/list.cgi", &[&ok]), route(path, &[OK])],
            &[],
            &[cmd],
        );
        assert!(r.ok, "{cmd}: {}", r.err);
        assert_eq!(r.hits(&format!("GET {path}")).len(), 1, "{:?}", r.reqs);
        assert!(r.hits("GET /axis-cgi/disks/format.cgi").is_empty());
    }
}

#[test]
fn a_camera_error_answer_becomes_an_error() {
    let ok = disk_xml("SD_DISK", "ext4", "OK", false);
    let err = "<root><GeneralError><ErrorCode>7</ErrorCode><ErrorDescription>Disk is in use</ErrorDescription></GeneralError></root>";
    let r = run("mount-error", vec![route("/axis-cgi/disks/list.cgi", &[&ok]), route("/axis-cgi/disks/unmount.cgi", &[err])], &[], &["unmount"]);
    assert!(!r.ok);
    assert!(r.err.contains("Disk is in use"), "{}", r.err);
}
