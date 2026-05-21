use clap::{Parser, Subcommand};

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
}

#[derive(Subcommand)]
pub enum Commands {
    /// Device info (model, firmware, serial)
    Info(cmd::info::InfoCmd),
    /// JPEG snapshot to file
    Snap(cmd::snap::SnapCmd),
    /// Firmware status
    Fw(cmd::fw::FwCmd),
    /// ACAP application management
    Acap(cmd::acap::AcapCmd),
    /// PTZ control (pan, tilt, zoom)
    Ptz(cmd::ptz::PtzCmd),
    /// Parameter management
    Param(cmd::param::ParamCmd),
    /// User account management
    User(cmd::user::UserCmd),
    /// Configuration management
    Config(cmd::config::ConfigCmd),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .without_time()
        .init();

    match cli.command {
        Commands::Info(cmd) => cmd.run(),
        Commands::Snap(cmd) => cmd.run(),
        Commands::Fw(cmd) => cmd.run(),
        Commands::Acap(cmd) => cmd.run(),
        Commands::Ptz(cmd) => cmd.run(),
        Commands::Param(cmd) => cmd.run(),
        Commands::User(cmd) => cmd.run(),
        Commands::Config(cmd) => cmd.run(),
    }
}
