use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::audio_clip;
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct ClipCmd {
    #[command(subcommand)]
    pub command: ClipCommands,
}

#[derive(Subcommand)]
pub enum ClipCommands {
    /// List audio clips stored on the camera
    List(ClipCameraArgs),
    /// Play an audio clip on the camera's built-in speaker
    Play(ClipPlayCmd),
    /// Upload an audio clip file to the camera
    Upload(ClipUploadCmd),
    /// Delete an audio clip from the camera
    Delete(ClipDeleteCmd),
}

#[derive(Args)]
pub struct ClipCameraArgs {
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
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct ClipPlayCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Clip name (with or without "clip:" prefix)
    pub name: String,
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

#[derive(Args)]
pub struct ClipUploadCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Path to the audio file to upload (.wav recommended)
    pub file: PathBuf,
    /// Override the clip name (default: filename without extension)
    #[arg(long)]
    pub name: Option<String>,
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

#[derive(Args)]
pub struct ClipDeleteCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Clip name to delete (with or without "clip:" prefix)
    pub name: String,
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

impl ClipCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ClipCommands::List(args) => {
                let client = make_client(&args.host, args.user.as_deref(), args.pass.as_deref(), args.port, args.insecure, args.timeout)?;
                let result = audio_clip::list_clips(&client)?;
                format::ok(&result);
                Ok(())
            }
            ClipCommands::Play(cmd) => {
                let client = make_client(&cmd.host, cmd.user.as_deref(), cmd.pass.as_deref(), cmd.port, cmd.insecure, cmd.timeout)?;
                audio_clip::play_clip(&client, &cmd.name)?;
                format::ok_msg(&format!("Playing clip: {}", cmd.name));
                Ok(())
            }
            ClipCommands::Upload(cmd) => {
                let filename = match &cmd.name {
                    Some(n) => {
                        // Append .wav if the override name has no extension
                        if std::path::Path::new(n).extension().is_some() {
                            n.clone()
                        } else {
                            format!("{}.wav", n)
                        }
                    }
                    None => cmd
                        .file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("clip.wav")
                        .to_string(),
                };
                let clip_name = std::path::Path::new(&filename)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&filename)
                    .to_string();

                let data = std::fs::read(&cmd.file).map_err(|e| {
                    anyhow::anyhow!("Cannot read file {}: {}", cmd.file.display(), e)
                })?;
                let client = make_client(&cmd.host, cmd.user.as_deref(), cmd.pass.as_deref(), cmd.port, cmd.insecure, cmd.timeout)?;
                audio_clip::upload_clip(&client, &data, &filename)?;
                format::ok_msg(&format!("Uploaded clip: {}", clip_name));
                Ok(())
            }
            ClipCommands::Delete(cmd) => {
                let client = make_client(&cmd.host, cmd.user.as_deref(), cmd.pass.as_deref(), cmd.port, cmd.insecure, cmd.timeout)?;
                audio_clip::delete_clip(&client, &cmd.name)?;
                let name = cmd.name.strip_prefix("clip:").unwrap_or(&cmd.name);
                format::ok_msg(&format!("Deleted clip: {}", name));
                Ok(())
            }
        }
    }
}

fn make_client(
    host: &str,
    user: Option<&str>,
    pass: Option<&str>,
    port: Option<u16>,
    insecure: bool,
    timeout: Option<u64>,
) -> anyhow::Result<VapixClient> {
    let (creds, resolved_host) = resolve(host, user, pass, port, insecure)?;
    let timeout = timeout.unwrap_or(creds.timeout);
    Ok(VapixClient::new(&resolved_host, creds.port, creds, timeout))
}
