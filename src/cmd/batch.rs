use std::process::Command;
use std::sync::Mutex;

use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::config::cameras::{self, CamerasConfig};
use crate::output::format;

#[derive(Args)]
pub struct BatchCmd {
    /// Camera group name, or comma-separated camera names/hosts
    pub targets: String,

    /// The vapx subcommand + arguments to run on each camera
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,

    /// Max parallel workers (default: number of targets)
    #[arg(short = 'j', long)]
    pub parallel: Option<usize>,

    /// Continue on error (don't stop on first failure)
    #[arg(long, default_value_t = true)]
    pub keep_going: bool,
}

impl BatchCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let config = cameras::load_cameras()?
            .ok_or_else(|| anyhow::anyhow!("No cameras.yaml found. Run `vapx config init` to create one."))?;

        let targets = resolve_targets(&config, &self.targets)?;

        if targets.is_empty() {
            anyhow::bail!("No cameras matched '{}'", self.targets);
        }

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.parallel.unwrap_or(targets.len()))
            .build()?;

        let results: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
        let vapx_bin = std::env::current_exe()?;

        let pb = ProgressBar::new(targets.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "));

        pool.install(|| {
            targets.par_iter().for_each(|camera_name| {
                pb.set_message(camera_name.clone());
                let mut cmd = Command::new(&vapx_bin);
                cmd.args(&self.command);
                cmd.arg(camera_name);

                let result = match cmd.output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

                        // Try to parse stdout as JSON for structured output
                        let data = serde_json::from_str::<serde_json::Value>(&stdout)
                            .unwrap_or_else(|_| serde_json::json!(stdout));

                        serde_json::json!({
                            "camera": camera_name,
                            "success": output.status.success(),
                            "result": data,
                            "stderr": if stderr.is_empty() { serde_json::Value::Null } else { serde_json::json!(stderr) },
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "camera": camera_name,
                            "success": false,
                            "result": null,
                            "stderr": e.to_string(),
                        })
                    }
                };

                results.lock().unwrap().push(result);
                pb.inc(1);
            });
        });

        pb.finish_and_clear();

        let results = results.into_inner().unwrap();
        let total = results.len();
        let ok_count = results.iter().filter(|r| r["success"].as_bool() == Some(true)).count();
        let fail_count = total - ok_count;

        format::ok(&serde_json::json!({
            "summary": {
                "total": total,
                "ok": ok_count,
                "failed": fail_count,
            },
            "results": results,
        }));

        Ok(())
    }
}

/// Resolve targets from group name or comma-separated camera names/hosts.
fn resolve_targets(config: &CamerasConfig, input: &str) -> anyhow::Result<Vec<String>> {
    // Check if it's a group name
    if let Some(members) = config.groups.get(input) {
        return Ok(members.clone());
    }

    // Treat as comma-separated camera names/hosts
    let names: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();

    // Validate all names exist in config
    for name in &names {
        if config.find(name).is_none() {
            anyhow::bail!("Camera '{}' not found in cameras.yaml", name);
        }
    }

    Ok(names)
}
