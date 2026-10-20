use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::streamurl;

#[derive(Args)]
pub struct StreamCmd {
    #[command(subcommand)]
    pub command: StreamCommands,
}

#[derive(Subcommand)]
pub enum StreamCommands {
    /// Generate RTSP stream URL
    Rtsp {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Generate MJPEG stream URL
    Mjpeg {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Generate JPEG snapshot URL
    Snapshot {
        #[command(flatten)]
        cam: CameraArgs,
    },
}

#[derive(Args, Clone)]
pub struct CameraArgs {
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

    /// Resolution (e.g. 1920x1080)
    #[arg(short, long)]
    pub resolution: Option<String>,

    /// Video codec: h264, h265, jpeg
    #[arg(long)]
    pub codec: Option<String>,

    /// Camera/video channel number (default: 1)
    #[arg(long, default_value_t = 1)]
    pub channel: u8,

    /// Stream profile number (default: 0)
    #[arg(long, default_value_t = 0)]
    pub stream_profile: u8,

    /// FPS limit
    #[arg(long)]
    pub fps: Option<u8>,

    /// Include credentials in URL
    #[arg(long)]
    pub with_creds: bool,

    /// Output as plain text (URL only, no JSON wrapper)
    #[arg(long)]
    pub plain: bool,
}

impl StreamCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            StreamCommands::Rtsp { cam } => build_rtsp(&cam),
            StreamCommands::Mjpeg { cam } => build_mjpeg(&cam),
            StreamCommands::Snapshot { cam } => build_snapshot(&cam),
        }
    }
}

fn opts(cam: &CameraArgs) -> streamurl::StreamOpts<'_> {
    streamurl::StreamOpts {
        channel: cam.channel,
        resolution: cam.resolution.as_deref(),
        codec: cam.codec.as_deref(),
        fps: cam.fps,
        stream_profile: cam.stream_profile,
    }
}

fn build_rtsp(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, host) = resolve(&cam.host, cam.user.as_deref(), cam.pass.as_deref(), cam.port, cam.insecure)?;
    let auth = cam.with_creds.then_some((creds.user.as_str(), creds.pass.as_str()));
    output_url(cam, &streamurl::rtsp(&host, auth, &opts(cam)), "rtsp");
    Ok(())
}

fn build_mjpeg(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, host) = resolve(&cam.host, cam.user.as_deref(), cam.pass.as_deref(), cam.port, cam.insecure)?;
    let auth = cam.with_creds.then_some((creds.user.as_str(), creds.pass.as_str()));
    output_url(cam, &streamurl::mjpeg(creds.https, &host, creds.port, auth, &opts(cam)), "mjpeg");
    Ok(())
}

fn build_snapshot(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, host) = resolve(&cam.host, cam.user.as_deref(), cam.pass.as_deref(), cam.port, cam.insecure)?;
    let auth = cam.with_creds.then_some((creds.user.as_str(), creds.pass.as_str()));
    output_url(cam, &streamurl::snapshot(creds.https, &host, creds.port, auth, &opts(cam)), "snapshot");
    Ok(())
}

fn output_url(cam: &CameraArgs, url: &str, stream_type: &str) {
    if cam.plain {
        println!("{}", url);
    } else {
        format::ok(&serde_json::json!({
            "type": stream_type,
            "url": url,
        }));
    }
}
