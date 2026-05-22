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
    /// Change user password
    Pass(cmd::pass::PassCmd),
    /// Network configuration
    Net(cmd::net::NetCmd),
    /// Time/NTP configuration
    Time(cmd::time::TimeCmd),
    /// I/O port management
    Hw(cmd::hw::HwCmd),
    /// Run command on multiple cameras
    Batch(cmd::batch::BatchCmd),
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
        Commands::Batch(cmd) => cmd.run(),
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
