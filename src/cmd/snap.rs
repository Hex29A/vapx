use std::path::PathBuf;

use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct SnapCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    /// Username
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    /// Password
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    /// Skip TLS certificate verification
    #[arg(short = 'k', long)]
    pub insecure: bool,

    /// Port number
    #[arg(long)]
    pub port: Option<u16>,

    /// Output file path (default: snapshot_<host>.jpg)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Resolution (e.g. 1920x1080, 640x480)
    #[arg(short, long)]
    pub resolution: Option<String>,

    /// Compression level 0-100 (higher = more compression)
    #[arg(short, long)]
    pub compression: Option<u8>,

    /// Interval between snapshots in seconds (enables time-lapse mode)
    #[arg(long)]
    pub interval: Option<u64>,

    /// Number of snapshots to capture (default: unlimited in time-lapse mode)
    #[arg(long)]
    pub count: Option<u64>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl SnapCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);

        let mut params: Vec<(&str, &str)> = Vec::new();
        let res_str;
        let comp_str;

        if let Some(ref r) = self.resolution {
            res_str = r.clone();
            params.push(("resolution", &res_str));
        }
        if let Some(c) = self.compression {
            comp_str = c.to_string();
            params.push(("compression", &comp_str));
        }

        // Time-lapse mode
        if let Some(interval) = self.interval {
            let max_count = self.count.unwrap_or(u64::MAX);
            let safe_host = resolved_host.replace(['.', ':'], "_");
            let mut captured = Vec::new();

            for i in 0..max_count {
                let bytes = client.get_bytes("/axis-cgi/jpg/image.cgi", &params)?;

                let output_path = if let Some(ref base) = self.output {
                    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = base.extension().unwrap_or_default().to_string_lossy();
                    let parent = base.parent().unwrap_or(std::path::Path::new("."));
                    parent.join(format!("{}_{:04}.{}", stem, i, if ext.is_empty() { "jpg" } else { &ext }))
                } else {
                    PathBuf::from(format!("snapshot_{}_{:04}.jpg", safe_host, i))
                };

                std::fs::write(&output_path, &bytes)?;
                eprintln!("[{}] {} ({})", i + 1, output_path.display(), format::human_bytes(bytes.len()));

                captured.push(serde_json::json!({
                    "file": output_path.display().to_string(),
                    "size": format::human_bytes(bytes.len()),
                    "bytes": bytes.len(),
                }));

                if i + 1 < max_count {
                    std::thread::sleep(std::time::Duration::from_secs(interval));
                }
            }

            format::ok(&serde_json::json!({
                "mode": "time-lapse",
                "interval_secs": interval,
                "captured": captured.len(),
                "files": captured,
            }));

            return Ok(());
        }

        // Single snapshot mode (original behavior)
        let bytes = client.get_bytes("/axis-cgi/jpg/image.cgi", &params)?;

        let output_path = self.output.unwrap_or_else(|| {
            let safe_host = resolved_host.replace(['.', ':'], "_");
            PathBuf::from(format!("snapshot_{}.jpg", safe_host))
        });

        std::fs::write(&output_path, &bytes)?;

        let info = serde_json::json!({
            "file": output_path.display().to_string(),
            "size": format::human_bytes(bytes.len()),
            "bytes": bytes.len(),
        });
        format::ok(&info);

        Ok(())
    }
}
