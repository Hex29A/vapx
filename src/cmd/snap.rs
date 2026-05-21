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
        println!("{}", serde_json::to_string_pretty(&info)?);

        Ok(())
    }
}
