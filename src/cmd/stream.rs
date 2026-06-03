use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;

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

fn build_rtsp(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, resolved_host) = resolve(
        &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
        cam.port, cam.insecure,
    )?;

    let codec = cam.codec.as_deref().unwrap_or("h264");
    let rtsp_port = 554;

    let auth_part = if cam.with_creds {
        format!("{}:{}@", creds.user, creds.pass)
    } else {
        String::new()
    };

    let mut path = format!("/axis-media/media.amp?videocodec={}&camera={}", codec, cam.channel);
    if let Some(ref res) = cam.resolution {
        path.push_str(&format!("&resolution={}", res));
    }
    if let Some(fps) = cam.fps {
        path.push_str(&format!("&fps={}", fps));
    }
    if cam.stream_profile > 0 {
        path.push_str(&format!("&streamprofile={}", cam.stream_profile));
    }

    let url = format!("rtsp://{}{}:{}{}", auth_part, resolved_host, rtsp_port, path);

    output_url(cam, &url, "rtsp");
    Ok(())
}

fn build_mjpeg(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, resolved_host) = resolve(
        &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
        cam.port, cam.insecure,
    )?;

    let scheme = if creds.https { "https" } else { "http" };

    let auth_part = if cam.with_creds {
        format!("{}:{}@", creds.user, creds.pass)
    } else {
        String::new()
    };

    let mut params = format!("camera={}", cam.channel);
    if let Some(ref res) = cam.resolution {
        params.push_str(&format!("&resolution={}", res));
    }
    if let Some(fps) = cam.fps {
        params.push_str(&format!("&fps={}", fps));
    }

    let url = format!(
        "{}://{}{}:{}/axis-cgi/mjpg/video.cgi?{}",
        scheme, auth_part, resolved_host, creds.port, params
    );

    output_url(cam, &url, "mjpeg");
    Ok(())
}

fn build_snapshot(cam: &CameraArgs) -> anyhow::Result<()> {
    let (creds, resolved_host) = resolve(
        &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
        cam.port, cam.insecure,
    )?;

    let scheme = if creds.https { "https" } else { "http" };

    let auth_part = if cam.with_creds {
        format!("{}:{}@", creds.user, creds.pass)
    } else {
        String::new()
    };

    let mut params = format!("camera={}", cam.channel);
    if let Some(ref res) = cam.resolution {
        params.push_str(&format!("&resolution={}", res));
    }

    let url = format!(
        "{}://{}{}:{}/axis-cgi/jpg/image.cgi?{}",
        scheme, auth_part, resolved_host, creds.port, params
    );

    output_url(cam, &url, "snapshot");
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
