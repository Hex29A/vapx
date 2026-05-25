use clap::{Args, Subcommand, ValueEnum};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::ptz;

#[derive(Args)]
pub struct PtzCmd {
    #[command(subcommand)]
    pub command: PtzCommands,
}

#[derive(Subcommand)]
pub enum PtzCommands {
    /// Move in a direction (home, up, down, left, right, stop)
    Move(PtzMoveCmd),
    /// Move to absolute/relative position (--pan, --tilt, --zoom in degrees/1-9999)
    Goto(PtzGotoCmd),
    /// Go to a named preset, or save current position as preset
    Preset(PtzPresetCmd),
    /// Query PTZ status (position, limits, presetposcam, presetposall, speed, status)
    Query(PtzQueryCmd),
    /// Show available PTZ commands
    Info(PtzInfoCmd),
}

#[derive(Clone, ValueEnum)]
pub enum Direction {
    Home,
    Up,
    Down,
    Left,
    Right,
    Upleft,
    Upright,
    Downleft,
    Downright,
    Stop,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Home => write!(f, "home"),
            Direction::Up => write!(f, "up"),
            Direction::Down => write!(f, "down"),
            Direction::Left => write!(f, "left"),
            Direction::Right => write!(f, "right"),
            Direction::Upleft => write!(f, "upleft"),
            Direction::Upright => write!(f, "upright"),
            Direction::Downleft => write!(f, "downleft"),
            Direction::Downright => write!(f, "downright"),
            Direction::Stop => write!(f, "stop"),
        }
    }
}

#[derive(Clone, ValueEnum)]
pub enum QueryType {
    Position,
    Limits,
    Presetposcam,
    Presetposall,
    Speed,
    Status,
    Attributes,
    Auxiliary,
}

impl std::fmt::Display for QueryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryType::Position => write!(f, "position"),
            QueryType::Limits => write!(f, "limits"),
            QueryType::Presetposcam => write!(f, "presetposcam"),
            QueryType::Presetposall => write!(f, "presetposall"),
            QueryType::Speed => write!(f, "speed"),
            QueryType::Status => write!(f, "status"),
            QueryType::Attributes => write!(f, "attributes"),
            QueryType::Auxiliary => write!(f, "auxiliary"),
        }
    }
}

#[derive(Args)]
pub struct PtzMoveCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Direction to move
    pub direction: Direction,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Video channel number
    #[arg(long)]
    pub camera: Option<u8>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct PtzGotoCmd {
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
    /// Absolute pan position (-180.0 to 180.0)
    #[arg(long, allow_hyphen_values = true)]
    pub pan: Option<f64>,
    /// Absolute tilt position (-180.0 to 180.0)
    #[arg(long, allow_hyphen_values = true)]
    pub tilt: Option<f64>,
    /// Absolute zoom position (1-9999)
    #[arg(long)]
    pub zoom: Option<i32>,
    /// Relative pan offset (-360.0 to 360.0)
    #[arg(long, allow_hyphen_values = true)]
    pub rpan: Option<f64>,
    /// Relative tilt offset (-360.0 to 360.0)
    #[arg(long, allow_hyphen_values = true)]
    pub rtilt: Option<f64>,
    /// Relative zoom offset (-9999 to 9999)
    #[arg(long, allow_hyphen_values = true)]
    pub rzoom: Option<i32>,
    /// Movement speed (1-100)
    #[arg(long)]
    pub speed: Option<i32>,
    /// Video channel number
    #[arg(long)]
    pub camera: Option<u8>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct PtzPresetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Preset name to go to (or save with --save)
    pub name: String,
    /// Save current position as this preset name instead of going to it
    #[arg(long)]
    pub save: bool,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Video channel number
    #[arg(long)]
    pub camera: Option<u8>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct PtzQueryCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// What to query
    pub what: QueryType,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Video channel number
    #[arg(long)]
    pub camera: Option<u8>,
    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct PtzInfoCmd {
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
    /// Video channel number
    #[arg(long)]
    pub camera: Option<u8>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl PtzCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            PtzCommands::Move(cmd) => cmd.run(),
            PtzCommands::Goto(cmd) => cmd.run(),
            PtzCommands::Preset(cmd) => cmd.run(),
            PtzCommands::Query(cmd) => cmd.run(),
            PtzCommands::Info(cmd) => cmd.run(),
        }
    }
}

impl PtzMoveCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        ptz::move_direction(&client, &self.direction.to_string(), self.camera)?;
        format::ok_msg("OK");
        Ok(())
    }
}

impl PtzGotoCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        ptz::goto(
            &client,
            ptz::GotoParams {
                pan: self.pan,
                tilt: self.tilt,
                zoom: self.zoom,
                rpan: self.rpan,
                rtilt: self.rtilt,
                rzoom: self.rzoom,
                speed: self.speed,
                camera: self.camera,
            },
        )?;
        format::ok_msg("OK");
        Ok(())
    }
}

impl PtzPresetCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        if self.save {
            ptz::save_preset(&client, &self.name, self.camera)?;
            format::ok_msg(&format!("Preset saved: {}", self.name));
        } else {
            ptz::goto_preset(&client, &self.name, self.camera)?;
            format::ok_msg("OK");
        }
        Ok(())
    }
}

impl PtzQueryCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let text = ptz::query(&client, &self.what.to_string(), self.camera)?;

        if self.plain {
            print!("{}", text);
        } else {
            let mut map = serde_json::Map::new();
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(
                        k.trim().to_string(),
                        serde_json::Value::String(v.trim().to_string()),
                    );
                }
            }
            format::ok(&map);
        }

        Ok(())
    }
}

impl PtzInfoCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let text = ptz::info(&client, self.camera)?;
        if text.trim().starts_with("Error:") || text.contains("PTZ disabled") {
            anyhow::bail!("{}", text.trim().trim_start_matches("Error:").trim());
        }
        print!("{}", text);
        Ok(())
    }
}
