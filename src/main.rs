use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod cmd;
mod config;
mod output;
mod vapix;

#[derive(Parser)]
#[command(name = "vapx", version, about = "Axis camera management CLI via VAPIX")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Filter output fields (comma-separated, e.g. "model,serial")
    #[arg(long, global = true)]
    pub filter: Option<String>,

    /// Output format: json (default), table, csv, yaml
    #[arg(long, global = true, default_value = "json")]
    pub format: String,

    /// Config profile to use (from cameras.yaml profiles section)
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Path to cameras.yaml config file
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Device info (model, firmware, serial)
    Info(cmd::info::InfoCmd),
    /// JPEG snapshot to file
    Snap(cmd::snap::SnapCmd),
    /// Firmware management
    Fw(cmd::fw::FwCmd),
    /// ACAP application management
    Acap(cmd::acap::AcapCmd),
    /// PTZ control (pan, tilt, zoom)
    Ptz(cmd::ptz::PtzCmd),
    /// Parameter management
    Param(cmd::param::ParamCmd),
    /// User account management
    User(cmd::user::UserCmd),
    /// Change user password
    Pass(cmd::pass::PassCmd),
    /// Network configuration
    Net(cmd::net::NetCmd),
    /// Time/NTP configuration
    Time(cmd::time::TimeCmd),
    /// I/O port management
    Hw(cmd::hw::HwCmd),
    /// Stream real-time events (motion, I/O, PTZ, etc.)
    Events(cmd::events::EventsCmd),
    /// Run command on multiple cameras
    Batch(cmd::batch::BatchCmd),
    /// Discover supported APIs on the camera
    Discover(cmd::discover::DiscoverCmd),
    /// Compare parameters between two cameras
    Diff(cmd::diff::DiffCmd),
    /// Backup and restore camera parameters
    Backup(cmd::backup::BackupCmd),
    /// Manage text/image overlays
    Overlay(cmd::overlay::OverlayCmd),
    /// View system/access logs
    Log(cmd::log::LogCmd),
    /// Generate stream URLs (RTSP, MJPEG, snapshot)
    Stream(cmd::stream::StreamCmd),
    /// Apply/create parameter templates (desired-state config)
    Template(cmd::template::TemplateCmd),
    /// Security posture audit
    Audit(cmd::audit::AuditCmd),
    /// Certificate management
    Cert(cmd::cert::CertCmd),
    /// Watch events from multiple cameras
    Watch(cmd::watch::WatchCmd),
    /// Action rule management
    Rule(cmd::rule::RuleCmd),
    /// Storage and SD card management
    Storage(cmd::storage::StorageCmd),
    /// Fleet health check
    Health(cmd::health::HealthCmd),
    /// Temperature sensor readings
    Temp(cmd::temp::TempCmd),
    /// Day/night IR-cut filter mode
    Daynight(cmd::daynight::DaynightCmd),
    /// Image sensor settings (brightness, contrast, exposure)
    Imaging(cmd::imaging::ImagingCmd),
    /// IR illuminator status and intensity
    Light(cmd::light::LightCmd),
    /// Video motion detection configuration
    Vmd(cmd::vmd::VmdCmd),
    /// Audio source configuration
    Audio(cmd::audio::AudioCmd),
    /// Configuration management
    Config(cmd::config::ConfigCmd),
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Generate man pages
    Mangen {
        /// Output directory for man pages
        #[arg(default_value = ".")]
        dir: std::path::PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    // Set output filter before running commands
    if let Some(ref filter) = cli.filter {
        let keys: Vec<String> = filter.split(',').map(|s| s.trim().to_string()).collect();
        output::format::set_filter(keys);
    }

    // Set output format
    if cli.format != "json" {
        output::format::set_output_format(cli.format.clone());
    }

    // Set explicit config path
    if let Some(ref config_path) = cli.config {
        config::cameras::set_config_path(config_path.clone());
    }

    // Set active config profile
    if let Some(ref profile) = cli.profile {
        config::cameras::set_profile(profile.clone());
    }

    let log_filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_filter)),
        )
        .with_target(false)
        .without_time()
        .init();

    let result = match cli.command {
        Commands::Info(cmd) => cmd.run(),
        Commands::Snap(cmd) => cmd.run(),
        Commands::Fw(cmd) => cmd.run(),
        Commands::Acap(cmd) => cmd.run(),
        Commands::Ptz(cmd) => cmd.run(),
        Commands::Param(cmd) => cmd.run(),
        Commands::User(cmd) => cmd.run(),
        Commands::Pass(cmd) => cmd.run(),
        Commands::Net(cmd) => cmd.run(),
        Commands::Time(cmd) => cmd.run(),
        Commands::Hw(cmd) => cmd.run(),
        Commands::Events(cmd) => cmd.run(),
        Commands::Batch(cmd) => cmd.run(),
        Commands::Discover(cmd) => cmd.run(),
        Commands::Diff(cmd) => cmd.run(),
        Commands::Backup(cmd) => cmd.run(),
        Commands::Overlay(cmd) => cmd.run(),
        Commands::Log(cmd) => cmd.run(),
        Commands::Stream(cmd) => cmd.run(),
        Commands::Template(cmd) => cmd.run(),
        Commands::Audit(cmd) => cmd.run(),
        Commands::Cert(cmd) => cmd.run(),
        Commands::Watch(cmd) => cmd.run(),
        Commands::Rule(cmd) => cmd.run(),
        Commands::Storage(cmd) => cmd.run(),
        Commands::Health(cmd) => cmd.run(),
        Commands::Temp(cmd) => cmd.run(),
        Commands::Daynight(cmd) => cmd.run(),
        Commands::Imaging(cmd) => cmd.run(),
        Commands::Light(cmd) => cmd.run(),
        Commands::Vmd(cmd) => cmd.run(),
        Commands::Audio(cmd) => cmd.run(),
        Commands::Config(cmd) => cmd.run(),
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "vapx", &mut std::io::stdout());
            Ok(())
        }
        Commands::Mangen { dir } => (|| -> anyhow::Result<()> {
            let cmd = Cli::command();
            std::fs::create_dir_all(&dir)?;
            clap_mangen::generate_to(cmd, &dir)
                .map_err(|e| anyhow::anyhow!("Failed to generate man pages: {}", e))?;
            eprintln!("Man pages written to {}", dir.display());
            Ok(())
        })(),
    };

    if let Err(e) = result {
        output::format::err_json("ERROR", &format!("{:#}", e));
    }
}
