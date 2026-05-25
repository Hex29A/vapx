use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::zipstream;

#[derive(Args)]
pub struct ZipstreamCmd {
    #[command(subcommand)]
    pub command: ZipstreamCommands,
}

#[derive(Subcommand)]
pub enum ZipstreamCommands {
    /// Show ZipStream profiles and settings
    Status(ZipstreamCameraArgs),
    /// Set ZipStream profile level
    Set(ZipstreamSetCmd),
}

#[derive(Args)]
pub struct ZipstreamCameraArgs {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct ZipstreamSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// ZipStream profile name (classic, storage, networkloadbalancing)
    #[arg(long)]
    pub profile: String,
    /// ZipStream strength level (0-100)
    #[arg(long)]
    pub level: u32,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl ZipstreamCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ZipstreamCommands::Status(args) => {
                let (creds, resolved_host) = resolve(
                    &args.host,
                    args.user.as_deref(),
                    args.pass.as_deref(),
                    args.port,
                    args.insecure,
                )?;
                let timeout = args.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let xml = zipstream::list_profiles(&client)?;

                if args.plain {
                    println!("{}", xml);
                } else {
                    let profiles = parse_zipstream_xml(&xml)?;
                    format::ok(&profiles);
                }
                Ok(())
            }
            ZipstreamCommands::Set(cmd) => {
                let (creds, resolved_host) = resolve(
                    &cmd.host,
                    cmd.user.as_deref(),
                    cmd.pass.as_deref(),
                    cmd.port,
                    cmd.insecure,
                )?;
                let timeout = cmd.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let _resp = zipstream::set_profile(&client, &cmd.profile, cmd.level)?;
                format::ok_msg(&format!(
                    "ZipStream profile '{}' set to level {}",
                    cmd.profile, cmd.level
                ));
                Ok(())
            }
        }
    }
}

fn parse_zipstream_xml(xml: &str) -> anyhow::Result<serde_json::Value> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    let mut profiles = Vec::new();

    for profile_node in root.descendants().filter(|n| n.has_tag_name("Profile")) {
        let mut profile = serde_json::Map::new();
        if let Some(name) = profile_node.attribute("name") {
            profile.insert("name".into(), serde_json::json!(name));
        }
        // Check for level attribute or child element
        if let Some(level) = profile_node.attribute("level") {
            profile.insert("level".into(), serde_json::json!(level));
        }
        for child in profile_node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if let Some(text) = child.text() {
                profile.insert(tag.to_string(), serde_json::json!(text.trim()));
            }
        }
        profiles.push(serde_json::Value::Object(profile));
    }

    // Also capture MaxLevel if present
    let mut result = serde_json::Map::new();
    if let Some(max_level_node) = root.descendants().find(|n| n.has_tag_name("MaxLevel")) {
        if let Some(text) = max_level_node.text() {
            result.insert(
                "maxLevel".into(),
                serde_json::json!(text.trim().parse::<i64>().unwrap_or(0)),
            );
        }
    }
    result.insert("profiles".into(), serde_json::json!(profiles));
    Ok(serde_json::Value::Object(result))
}
