use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clap::Args;

use crate::config::cameras::{self, CamerasConfig};
use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::events;

#[derive(Args)]
pub struct WatchCmd {
    /// Camera group or comma-separated camera names/hosts
    pub targets: String,

    /// ONVIF topic filter (e.g. "tns1:Device/tnsaxis:IO/VirtualPort")
    #[arg(short, long)]
    pub topic: Option<String>,

    /// Max total events to receive across all cameras (0 = unlimited)
    #[arg(short = 'n', long, default_value_t = 0)]
    pub count: u64,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl WatchCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let config = cameras::load_cameras()?
            .ok_or_else(|| anyhow::anyhow!("No cameras.yaml found. Run `vapx config init` to create one."))?;

        let targets = resolve_targets(&config, &self.targets)?;
        if targets.is_empty() {
            anyhow::bail!("No cameras matched '{}'", self.targets);
        }

        // For single camera, just stream inline
        if targets.len() == 1 {
            let name = &targets[0];
            let (creds, resolved_host) = resolve(name, None, None, None, false)?;
            let timeout = self.timeout.unwrap_or(creds.timeout);
            let client = VapixClient::new(&resolved_host, creds.port, creds.clone(), timeout);

            let mut received = 0u64;
            let max_count = self.count;

            events::stream_events(
                &client,
                &creds,
                &resolved_host,
                self.topic.as_deref(),
                |event| {
                    let notification = event.pointer("/params/notification").unwrap_or(event);
                    let output = serde_json::json!({
                        "camera": name,
                        "event": notification,
                    });
                    println!("{}", serde_json::to_string(&output).unwrap_or_default());

                    received += 1;
                    if max_count > 0 && received >= max_count {
                        return false;
                    }
                    true
                },
            )?;
            return Ok(());
        }

        // For multiple cameras, spawn threads
        let mut handles = Vec::new();
        let total_received = Arc::new(AtomicU64::new(0));
        let max_count = self.count;

        for name in &targets {
            let name = name.clone();
            let topic = self.topic.clone();
            let total_c = total_received.clone();
            let timeout = self.timeout;

            let handle = std::thread::spawn(move || -> anyhow::Result<()> {
                let (creds, resolved_host) = resolve(&name, None, None, None, false)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds.clone(), t);

                events::stream_events(
                    &client,
                    &creds,
                    &resolved_host,
                    topic.as_deref(),
                    |event| {
                        let notification = event.pointer("/params/notification").unwrap_or(event);
                        let output = serde_json::json!({
                            "camera": name,
                            "event": notification,
                        });
                        println!("{}", serde_json::to_string(&output).unwrap_or_default());

                        let prev = total_c.fetch_add(1, Ordering::Relaxed);
                        if max_count > 0 && prev + 1 >= max_count {
                            return false;
                        }
                        true
                    },
                )?;
                Ok(())
            });
            handles.push(handle);
        }

        for handle in handles {
            if let Err(e) = handle.join().unwrap_or_else(|_| Err(anyhow::anyhow!("Thread panicked"))) {
                eprintln!("Watch error: {}", e);
            }
        }

        Ok(())
    }
}

fn resolve_targets(config: &CamerasConfig, input: &str) -> anyhow::Result<Vec<String>> {
    if let Some(members) = config.groups.get(input) {
        return Ok(members.clone());
    }
    let names: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();
    for name in &names {
        if config.find(name).is_none() {
            anyhow::bail!("Camera '{}' not found in cameras.yaml", name);
        }
    }
    Ok(names)
}
