use std::process::Command;
use std::sync::Mutex;

use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::config::cameras;
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

        let targets = crate::cmd::resolve_targets(&config, &self.targets)?;

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

        // A --format among the command's arguments is for the batch result, not for each
        // camera: every camera answers JSON, so that its result can be read and combined.
        let (command, trailing_format) = split_format(&self.command);
        let fmt = trailing_format.unwrap_or_else(|| format::current_format().to_string());

        pool.install(|| {
            targets.par_iter().for_each(|camera_name| {
                pb.set_message(camera_name.clone());
                let mut cmd = Command::new(&vapx_bin);
                cmd.args(&command);
                cmd.args(["--format", "json"]);
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

        let mut results = results.into_inner().unwrap();
        // The order the cameras were given in, not the order they answered in.
        results.sort_by_key(|r| targets.iter().position(|t| r["camera"] == t.as_str()).unwrap_or(usize::MAX));
        let total = results.len();
        let ok_count = results.iter().filter(|r| r["success"].as_bool() == Some(true)).count();
        let fail_count = total - ok_count;

        if fmt == "csv" || fmt == "table" {
            let (headers, rows) = table_of(&results);
            format::print_rows(&fmt, &headers, &rows);
            return Ok(());
        }
        format::ok_as(&fmt, &serde_json::json!({
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

/// The command without its `--format X` / `--format=X`, and that format if there was one.
fn split_format(args: &[String]) -> (Vec<String>, Option<String>) {
    let mut out = Vec::new();
    let mut found = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--format" {
            found = it.next().cloned();
        } else if let Some(v) = a.strip_prefix("--format=") {
            found = Some(v.to_string());
        } else {
            out.push(a.clone());
        }
    }
    (out, found)
}

/// Flatten an answer into dotted columns: {"a": {"b": 1}} gives a.b = 1. Arrays stay one cell.
fn flatten(prefix: &str, value: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&key, v, out);
            }
        }
        other => out.push((if prefix.is_empty() { "result".to_string() } else { prefix.to_string() }, other.clone())),
    }
}

/// One row per camera: camera, success, the fields of its answer, and error last.
fn table_of(results: &[serde_json::Value]) -> (Vec<String>, Vec<Vec<String>>) {
    let mut per_camera = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    for r in results {
        let answer = &r["result"];
        // A camera answers {"status": "ok", "data": ...}; the data is what goes in the row.
        let data = answer.get("data").unwrap_or(answer);
        let mut fields = Vec::new();
        if !data.is_null() && data.as_str() != Some("") {
            flatten("", data, &mut fields);
        }
        for (k, _) in &fields {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
        let error = r["stderr"]
            .as_str()
            .map(|e| serde_json::from_str::<serde_json::Value>(e).ok().and_then(|v| v["message"].as_str().map(String::from)).unwrap_or_else(|| e.to_string()))
            .filter(|_| r["success"] != true)
            .unwrap_or_default();
        per_camera.push((r["camera"].as_str().unwrap_or("").to_string(), r["success"] == true, fields, error));
    }
    let mut headers = vec!["camera".to_string(), "success".to_string()];
    headers.extend(columns.iter().cloned());
    headers.push("error".to_string());
    let rows = per_camera
        .into_iter()
        .map(|(camera, success, fields, error)| {
            let mut row = vec![camera, success.to_string()];
            for c in &columns {
                row.push(fields.iter().find(|(k, _)| k == c).map(|(_, v)| format::cell(v)).unwrap_or_default());
            }
            row.push(error);
            row
        })
        .collect();
    (headers, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_format_in_the_command_is_taken_out_for_the_batch() {
        let args: Vec<String> = ["o3c", "oak", "--format", "csv"].iter().map(|s| s.to_string()).collect();
        let (cmd, fmt) = split_format(&args);
        assert_eq!(cmd, vec!["o3c", "oak"]);
        assert_eq!(fmt.as_deref(), Some("csv"));
        let args: Vec<String> = ["--format=table", "info"].iter().map(|s| s.to_string()).collect();
        assert_eq!(split_format(&args), (vec!["info".to_string()], Some("table".to_string())));
    }

    #[test]
    fn each_camera_is_one_row_with_its_answer_flattened() {
        let results = vec![
            json!({"camera": "a", "success": true, "result": {"status": "ok", "data": {"serial": "S1", "oak": "K1"}}, "stderr": null}),
            json!({"camera": "b", "success": false, "result": "", "stderr": "{\"status\":\"error\",\"message\":\"HTTP 401\"}"}),
            json!({"camera": "c", "success": true, "result": {"status": "ok", "data": {"disk": {"id": "SD_DISK"}}}, "stderr": "a warning"}),
        ];
        let (headers, rows) = table_of(&results);
        assert_eq!(headers, vec!["camera", "success", "oak", "serial", "disk.id", "error"]);
        assert_eq!(rows[0], vec!["a", "true", "K1", "S1", "", ""]);
        assert_eq!(rows[1], vec!["b", "false", "", "", "", "HTTP 401"]);
        assert_eq!(rows[2], vec!["c", "true", "", "", "SD_DISK", ""]);
    }
}
