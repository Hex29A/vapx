//! Installing and upgrading ACAP applications (`applications/upload.cgi`), license keys
//! (`applications/license.cgi`) and the AllowUnsigned setting (`applications/config.cgi`).
//!
//! The camera installs whatever it is given: it does not compare versions, so an older
//! package silently downgrades the app. vapx reads the package first and says what will
//! happen (install, upgrade, reinstall or downgrade) before anything is sent.

use std::cmp::Ordering;
use std::io::Read;

use anyhow::{bail, Context};
use serde_json::{json, Value};

use crate::vapix::applications::{self, AcapApp};
use crate::vapix::client::VapixClient;
use crate::vapix::params;

const UPLOAD: &str = "/axis-cgi/applications/upload.cgi";
const LICENSE: &str = "/axis-cgi/applications/license.cgi";
const CONFIG: &str = "/axis-cgi/applications/config.cgi";
/// Packages larger than this are refused before reading them into memory.
pub const MAX_PACKAGE_BYTES: u64 = 512 << 20;

/// What a .eap package says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The package name: the key the camera installs under (`appName` / `APPNAME`).
    pub name: String,
    pub nice_name: String,
    pub version: String,
    /// `aarch64`, `armv7hf`, `mipsisa32r2el`, or empty/`all` for any architecture.
    pub arch: String,
    pub vendor: String,
    /// The file it came from, for messages.
    pub source: String,
}

impl Package {
    pub fn fits(&self, camera_arch: &str) -> bool {
        self.arch.is_empty() || self.arch.eq_ignore_ascii_case("all") || camera_arch.is_empty() || self.arch.eq_ignore_ascii_case(camera_arch)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "niceName": self.nice_name,
            "version": self.version,
            "architecture": if self.arch.is_empty() { "all" } else { self.arch.as_str() },
            "vendor": self.vendor,
            "file": self.source,
        })
    }
}

/// Read a .eap (a gzip-compressed tar) and find manifest.json (ACAP 4) or package.conf (ACAP 3).
pub fn read_package(data: &[u8], source: &str) -> anyhow::Result<Package> {
    let gz = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(gz);
    let mut manifest = None;
    let mut conf = None;
    let entries = archive.entries().with_context(|| format!("{source} is not an ACAP package (.eap: a gzip-compressed tar)"))?;
    for entry in entries {
        let mut entry = entry.with_context(|| format!("{source} is not a readable .eap"))?;
        let path = entry.path()?.to_string_lossy().trim_start_matches("./").to_string();
        if path == "manifest.json" || path == "package.conf" {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            if path == "manifest.json" {
                manifest = Some(text);
            } else {
                conf = Some(text);
            }
        }
    }
    let mut pkg = if let Some(m) = manifest {
        from_manifest(&m)?
    } else if let Some(c) = conf {
        from_package_conf(&c)
    } else {
        bail!("{source} has neither manifest.json nor package.conf, so it is not an ACAP package");
    };
    if pkg.name.is_empty() {
        bail!("{source}: the package does not name its application");
    }
    pkg.source = source.to_string();
    Ok(pkg)
}

fn from_manifest(text: &str) -> anyhow::Result<Package> {
    let v: Value = serde_json::from_str(text).context("manifest.json is not valid JSON")?;
    let setup = v.pointer("/acapPackageConf/setup").cloned().unwrap_or(Value::Null);
    let s = |k: &str| setup.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Package {
        name: s("appName"),
        nice_name: s("friendlyName"),
        version: s("version"),
        arch: s("architecture"),
        vendor: s("vendor"),
        source: String::new(),
    })
}

fn from_package_conf(text: &str) -> Package {
    let mut kv = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
        }
    }
    let g = |k: &str| kv.get(k).cloned().unwrap_or_default();
    let version = [g("APPMAJORVERSION"), g("APPMINORVERSION"), g("APPMICROVERSION")]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(".");
    Package { name: g("APPNAME"), nice_name: g("PACKAGENAME"), version, arch: g("APPTYPE"), vendor: g("VENDOR"), source: String::new() }
}

/// Compare versions the way people read them: numbers left to right, so 1.10 > 1.9.
/// The camera writes the last part after a dash (4.5-1), the package with a dot (4.5.1).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<u64> { s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect() };
    let (mut x, mut y) = (parts(a), parts(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x.cmp(&y)
}

/// What uploading the package would do on this camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Install,
    Upgrade,
    Reinstall,
    Downgrade,
}

impl Plan {
    pub fn name(self) -> &'static str {
        match self {
            Plan::Install => "install",
            Plan::Upgrade => "upgrade",
            Plan::Reinstall => "reinstall",
            Plan::Downgrade => "downgrade",
        }
    }
}

pub fn plan(pkg: &Package, installed: Option<&AcapApp>) -> Plan {
    match installed {
        None => Plan::Install,
        Some(app) => match compare_versions(&pkg.version, &app.version) {
            Ordering::Greater => Plan::Upgrade,
            Ordering::Equal => Plan::Reinstall,
            Ordering::Less => Plan::Downgrade,
        },
    }
}

/// The camera's architecture (`Properties.System.Architecture`), empty when it does not say.
pub fn camera_arch(client: &VapixClient) -> String {
    params::list(client, Some("root.Properties.System.Architecture"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.split_once('=').map(|(_, v)| v.trim().to_string())))
        .unwrap_or_default()
}

/// Pick the package for this camera's architecture among several.
pub fn choose<'a>(packages: &'a [Package], arch: &str) -> anyhow::Result<&'a Package> {
    let fitting: Vec<&Package> = packages.iter().filter(|p| p.fits(arch)).collect();
    // A package built for exactly this architecture wins over one for all.
    let exact: Vec<&&Package> = fitting.iter().filter(|p| p.arch.eq_ignore_ascii_case(arch)).collect();
    match (exact.len(), fitting.len()) {
        (1, _) => Ok(exact[0]),
        (0, 1) => Ok(fitting[0]),
        (0, 0) => {
            let have: Vec<String> = packages.iter().map(|p| format!("{} ({})", p.source, if p.arch.is_empty() { "all" } else { &p.arch })).collect();
            bail!("no package fits this camera's architecture {}: {}", if arch.is_empty() { "(unknown)" } else { arch }, have.join(", "))
        }
        _ => bail!("more than one package fits architecture {arch}; give only one per architecture"),
    }
}

/// What an `Error: N` answer from upload.cgi means.
pub fn upload_error(code: &str) -> String {
    let what = match code {
        "1" => "the file is not an ACAP package (.eap)",
        "2" => "the package signature is missing or not valid. Unsigned apps are refused from AXIS OS 12; see `vapx acap unsigned`",
        "3" => "the package is too large, or the camera's storage is full",
        "5" => "the package is not compatible with this camera (architecture, SDK or AXIS OS version); the system log says which",
        "10" => "unspecified error; the camera's system log has the details",
        "12" => "another upload is in progress on the camera; try again shortly",
        "13" => "the package asks for a user or group the camera does not allow (root-privileged apps are refused from AXIS OS 11.8)",
        "14" => "an app with the same name in different letter case is installed",
        "15" => "the camera timed out and the outcome is unknown; check `vapx acap list`",
        "27" => "the package comes from a different vendor than the app it would replace",
        "29" => "the package's manifest.json or package.conf is not valid",
        "51" | "52" | "53" | "54" => "a Developer Mode restriction on this camera refused the package",
        _ => "unknown error code",
    };
    format!("the camera refused the package (error {code}): {what}")
}

/// Upload the package. The camera installs it (or replaces the app with the same name),
/// keeps the app's data and parameters, and restarts it if it was running.
pub fn upload(client: &VapixClient, data: &[u8], filename: &str) -> anyhow::Result<()> {
    let text = client.post_multipart_file_with_params(UPLOAD, &[], data, filename, "file")?;
    let t = text.trim();
    if t == "OK" || t.is_empty() {
        return Ok(());
    }
    if let Some(code) = t.strip_prefix("Error:").map(str::trim) {
        bail!("{}", upload_error(code));
    }
    bail!("unexpected answer from upload.cgi: {}", t.chars().take(200).collect::<String>())
}

pub fn find_app(client: &VapixClient, name: &str) -> anyhow::Result<Option<AcapApp>> {
    Ok(applications::list_applications(client)?.into_iter().find(|a| a.name == name))
}

fn license_error(code: &str) -> &'static str {
    match code {
        "21" => "the license key file is not valid",
        "22" => "the upload failed; see the camera's system log",
        "23" => "the license key could not be removed; see the camera's system log",
        "24" => "the application is not installed",
        "25" => "the key is for a different application",
        "26" => "the key cannot be used with this version of the application",
        "30" => "the key is for a different camera (serial number)",
        "31" => "the license has expired",
        _ => "unknown error code",
    }
}

fn check_license_answer(text: &str) -> anyhow::Result<()> {
    let t = text.trim();
    if t == "OK" || t.is_empty() {
        return Ok(());
    }
    if let Some(code) = t.strip_prefix("Error:").map(str::trim) {
        bail!("the camera refused (error {code}): {}", license_error(code));
    }
    bail!("unexpected answer from license.cgi: {}", t.chars().take(200).collect::<String>())
}

/// Install a license key file for an installed application.
pub fn upload_license(client: &VapixClient, package: &str, key: &[u8], filename: &str) -> anyhow::Result<()> {
    let text = client.post_multipart_file_with_params(
        LICENSE,
        &[("action", "uploadlicensekey"), ("package", package)],
        key,
        filename,
        "fileData",
    )?;
    check_license_answer(&text)
}

pub fn remove_license(client: &VapixClient, package: &str) -> anyhow::Result<()> {
    let text = client.get_text(LICENSE, &[("action", "removelicensekey"), ("package", package)])?;
    check_license_answer(&text)
}

/// Read a config.cgi answer: `<reply result="ok"><param name=".." value=".."/>` or an error.
fn config_value(xml: &str) -> anyhow::Result<Option<String>> {
    let doc = roxmltree::Document::parse(xml.trim()).context("config.cgi did not answer XML")?;
    let reply = doc.descendants().find(|n| n.has_tag_name("reply"));
    if reply.and_then(|r| r.attribute("result")) == Some("error") {
        let msg = doc.descendants().find(|n| n.has_tag_name("error")).and_then(|e| e.attribute("message")).unwrap_or("unknown error");
        bail!("config.cgi: {msg}");
    }
    Ok(doc.descendants().find(|n| n.has_tag_name("param")).and_then(|p| p.attribute("value")).map(str::to_string))
}

fn config_call(client: &VapixClient, query: &[(&str, &str)]) -> anyhow::Result<Option<String>> {
    match client.get_text(CONFIG, query) {
        Ok(body) => config_value(&body),
        Err(e) if e.to_string().contains("HTTP 404") => {
            bail!("this camera has no applications/config.cgi (it needs AXIS OS 11.2 or later); unsigned apps are allowed there")
        }
        Err(e) => Err(e),
    }
}

/// Whether the camera installs unsigned applications.
pub fn allow_unsigned(client: &VapixClient) -> anyhow::Result<bool> {
    let v = config_call(client, &[("action", "get"), ("name", "AllowUnsigned")])?;
    Ok(v.as_deref() == Some("true"))
}

/// Allow or refuse unsigned applications, and read it back.
pub fn set_allow_unsigned(client: &VapixClient, allow: bool) -> anyhow::Result<bool> {
    let value = if allow { "true" } else { "false" };
    config_call(client, &[("action", "set"), ("name", "AllowUnsigned"), ("value", value)])?;
    let now = allow_unsigned(client)?;
    if now != allow {
        bail!("the camera reports AllowUnsigned={now} after setting {value}");
    }
    Ok(now)
}

/// A package and its bytes, from a local file or an https URL.
pub struct Loaded {
    pub package: Package,
    pub data: Vec<u8>,
    pub filename: String,
}

pub fn load(source: &str) -> anyhow::Result<Loaded> {
    let (data, filename) = if source.starts_with("https://") || source.starts_with("http://") {
        let resp = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()?
            .get(source)
            .send()
            .with_context(|| format!("cannot download {source}"))?;
        if !resp.status().is_success() {
            bail!("cannot download {source}: HTTP {}", resp.status().as_u16());
        }
        if resp.content_length().is_some_and(|n| n > MAX_PACKAGE_BYTES) {
            bail!("{source} is larger than {} MB", MAX_PACKAGE_BYTES >> 20);
        }
        let bytes = resp.bytes()?.to_vec();
        let name = source.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("app.eap").split('?').next().unwrap_or("app.eap").to_string();
        (bytes, name)
    } else {
        let path = std::path::Path::new(source);
        let meta = std::fs::metadata(path).with_context(|| format!("package not found: {source}"))?;
        if !meta.is_file() {
            bail!("{source} is not a regular file");
        }
        if meta.len() > MAX_PACKAGE_BYTES {
            bail!("{source} is larger than {} MB", MAX_PACKAGE_BYTES >> 20);
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "app.eap".into());
        (std::fs::read(path).with_context(|| format!("cannot read {source}"))?, name)
    };
    let package = read_package(&data, source)?;
    Ok(Loaded { package, data, filename })
}

/// How an install should behave.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Allow installing a version older than the installed one.
    pub allow_downgrade: bool,
    /// Only replace an installed app; refuse a fresh install.
    pub upgrade_only: bool,
    /// Start the app afterwards if it is not running.
    pub start: bool,
    /// Say what would happen, change nothing.
    pub dry_run: bool,
}

/// Install or upgrade the fitting package on one camera and check the result.
pub fn install(client: &VapixClient, loaded: &[Loaded], opts: Options) -> anyhow::Result<Value> {
    if loaded.is_empty() {
        bail!("no package given");
    }
    let arch = camera_arch(client);
    let packages: Vec<Package> = loaded.iter().map(|l| l.package.clone()).collect();
    let pkg = choose(&packages, &arch)?.clone();
    let chosen = loaded.iter().find(|l| l.package == pkg).expect("chosen package is among the loaded ones");
    let before = find_app(client, &pkg.name)?;
    let plan = plan(&pkg, before.as_ref());
    let installed_version = before.as_ref().map(|a| a.version.clone());
    if opts.upgrade_only && before.is_none() {
        bail!("{} is not installed on this camera, so there is nothing to upgrade; use `vapx acap install`", pkg.name);
    }
    if plan == Plan::Downgrade && !opts.allow_downgrade {
        bail!(
            "{} {} is installed and the package is the older {}. The camera would install it anyway (it keeps the app's data); run again with --allow-downgrade to do so",
            pkg.name,
            installed_version.as_deref().unwrap_or("?"),
            pkg.version
        );
    }
    let mut out = json!({
        "cameraArchitecture": if arch.is_empty() { Value::Null } else { json!(arch) },
        "package": pkg.to_json(),
        "plan": plan.name(),
        "installedVersion": installed_version,
    });
    if opts.dry_run {
        out["performed"] = json!(false);
        out["note"] = json!("Nothing was changed.");
        return Ok(out);
    }
    upload(client, &chosen.data, &chosen.filename)?;
    let mut after = find_app(client, &pkg.name)?.with_context(|| format!("the camera accepted the package but does not list {}", pkg.name))?;
    if !pkg.version.is_empty() && compare_versions(&after.version, &pkg.version) != Ordering::Equal {
        bail!("the camera lists {} {} after installing {}", pkg.name, after.version, pkg.version);
    }
    let mut started = false;
    if opts.start && !after.status.eq_ignore_ascii_case("running") {
        applications::control(client, "start", &pkg.name)?;
        started = true;
        if let Some(a) = find_app(client, &pkg.name)? {
            after = a;
        }
    }
    out["performed"] = json!(true);
    out["version"] = json!(after.version);
    out["status"] = json!(after.status);
    out["signature"] = json!(after.signature);
    out["started"] = json!(started);
    if !opts.start && plan == Plan::Install {
        out["note"] = json!("A new install is not started; use `vapx acap start` or --start.");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn eap(files: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (name, body) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            builder.append_data(&mut h, name, body.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    const MANIFEST: &str = r#"{"schemaVersion":"1.3","acapPackageConf":{"setup":{"appName":"hello","friendlyName":"Hello","vendor":"Example","version":"1.2.3","architecture":"aarch64"}}}"#;

    #[test]
    fn a_manifest_package_is_read() {
        let p = read_package(&eap(&[("./manifest.json", MANIFEST), ("hello", "bin")]), "hello.eap").unwrap();
        assert_eq!((p.name.as_str(), p.version.as_str(), p.arch.as_str(), p.nice_name.as_str()), ("hello", "1.2.3", "aarch64", "Hello"));
    }

    #[test]
    fn a_package_conf_package_is_read() {
        let conf = "PACKAGENAME=\"Old App\"\nAPPNAME=\"oldapp\"\nAPPTYPE=\"armv7hf\"\nAPPMAJORVERSION=\"2\"\nAPPMINORVERSION=\"10\"\nAPPMICROVERSION=\"0\"\nVENDOR=\"Example\"\n";
        let p = read_package(&eap(&[("package.conf", conf)]), "old.eap").unwrap();
        assert_eq!((p.name.as_str(), p.version.as_str(), p.arch.as_str()), ("oldapp", "2.10.0", "armv7hf"));
    }

    #[test]
    fn a_file_that_is_not_a_package_is_refused() {
        assert!(read_package(b"not a tarball", "x.eap").is_err());
        assert!(read_package(&eap(&[("readme.txt", "hi")]), "x.eap").unwrap_err().to_string().contains("not an ACAP package"));
    }

    #[test]
    fn versions_compare_by_number_and_ignore_the_dash() {
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("4.5.1", "4.5-1"), Ordering::Equal);
        assert_eq!(compare_versions("4.5", "4.5.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0", "10.0"), Ordering::Less);
    }

    fn app(version: &str) -> AcapApp {
        AcapApp {
            name: "hello".into(),
            nice_name: String::new(),
            vendor: String::new(),
            version: version.into(),
            status: "Running".into(),
            license: String::new(),
            license_name: String::new(),
            signature: String::new(),
            config_page: String::new(),
            resources: vec![],
        }
    }

    #[test]
    fn the_plan_follows_the_installed_version() {
        let p = read_package(&eap(&[("manifest.json", MANIFEST)]), "h.eap").unwrap();
        assert_eq!(plan(&p, None), Plan::Install);
        assert_eq!(plan(&p, Some(&app("1.2-2"))), Plan::Upgrade);
        assert_eq!(plan(&p, Some(&app("1.2-3"))), Plan::Reinstall);
        assert_eq!(plan(&p, Some(&app("1.3.0"))), Plan::Downgrade);
    }

    #[test]
    fn the_package_for_the_camera_architecture_is_chosen() {
        let mk = |arch: &str, src: &str| Package { name: "a".into(), nice_name: String::new(), version: "1".into(), arch: arch.into(), vendor: String::new(), source: src.into() };
        let pkgs = vec![mk("aarch64", "a64.eap"), mk("armv7hf", "arm.eap")];
        assert_eq!(choose(&pkgs, "armv7hf").unwrap().source, "arm.eap");
        assert_eq!(choose(&pkgs, "aarch64").unwrap().source, "a64.eap");
        assert!(choose(&pkgs, "mipsisa32r2el").unwrap_err().to_string().contains("a64.eap (aarch64)"));
        let any = vec![mk("all", "any.eap"), mk("armv7hf", "arm.eap")];
        assert_eq!(choose(&any, "armv7hf").unwrap().source, "arm.eap");
        assert_eq!(choose(&any, "aarch64").unwrap().source, "any.eap");
    }

    #[test]
    fn upload_errors_say_what_went_wrong() {
        assert!(upload_error("2").contains("signature"));
        assert!(upload_error("5").contains("not compatible"));
        assert!(upload_error("99").contains("unknown"));
    }

    #[test]
    fn config_answers_are_read() {
        assert_eq!(config_value(r#"<reply result="ok"><param name="AllowUnsigned" value="false" /></reply>"#).unwrap().as_deref(), Some("false"));
        assert!(config_value(r#"<reply result="error"><error type="1" message="bad" /></reply>"#).is_err());
    }
}
