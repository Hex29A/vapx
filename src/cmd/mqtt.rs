use clap::{Args, Subcommand};

use crate::cmd::CameraArgs as MqttCameraArgs;
use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::mqtt;
use serde_json::Value;

#[derive(Args)]
pub struct MqttCmd {
    #[command(subcommand)]
    pub command: MqttCommands,
}

#[derive(Subcommand)]
pub enum MqttCommands {
    /// Show MQTT client status and configuration
    Status(MqttCameraArgs),
    /// Configure MQTT broker connection
    Configure(MqttConfigureCmd),
    /// Show or modify event publication configuration
    Events(MqttEventsCmd),
    /// Enable (activate) MQTT client
    Enable(MqttCameraArgs),
    /// Disable (deactivate) MQTT client
    Disable(MqttCameraArgs),
}

/// `vapx mqtt events <host>` shows the current event publication config.
/// Passing any of `--add`, `--remove`, or `--clear` switches to read-modify-write
/// mode: the current config is fetched, only its `eventFilterList` is changed, and
/// the full config is written back so unrelated fields are preserved.
#[derive(Args)]
pub struct MqttEventsCmd {
    /// Camera connection
    #[command(flatten)]
    pub cam: MqttCameraArgs,
    /// Topic filter to add (repeatable). Idempotent: a filter with the same
    /// topicFilter is not duplicated.
    #[arg(long = "add", value_name = "TOPIC")]
    pub add: Vec<String>,
    /// Topic filter to remove (repeatable)
    #[arg(long = "remove", value_name = "TOPIC")]
    pub remove: Vec<String>,
    /// Remove all existing filters before applying --add
    #[arg(long)]
    pub clear: bool,
    /// QoS (0-2) applied to filters added with --add
    #[arg(long, default_value = "0", value_parser = clap::value_parser!(u8).range(0..=2))]
    pub qos: u8,
    /// Retain mode applied to filters added with --add
    #[arg(long, default_value = "none", value_parser = ["none", "property", "all"])]
    pub retain: String,
}

#[derive(Args)]
pub struct MqttConfigureCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// MQTT broker host
    #[arg(long)]
    pub broker: String,
    /// MQTT broker port (default: 1883)
    #[arg(long, default_value = "1883")]
    pub broker_port: u16,
    /// MQTT protocol (tcp or ssl)
    #[arg(long, default_value = "tcp")]
    pub protocol: String,
    /// MQTT client ID (defaults to camera-generated ID)
    #[arg(long)]
    pub client_id: Option<String>,
    /// MQTT username for broker authentication
    #[arg(long)]
    pub mqtt_user: Option<String>,
    /// MQTT password for broker authentication
    #[arg(long)]
    pub mqtt_pass: Option<String>,
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

impl MqttCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            MqttCommands::Status(args) => {
                let client = make_client(&args)?;
                let result = mqtt::get_client_status(&client)?;
                format::ok(&result.get("data").unwrap_or(&result));
                Ok(())
            }
            MqttCommands::Configure(cmd) => cmd.run(),
            MqttCommands::Events(cmd) => cmd.run(),
            MqttCommands::Enable(args) => {
                let client = make_client(&args)?;
                mqtt::activate_client(&client)?;
                format::ok_msg("MQTT client activated");
                Ok(())
            }
            MqttCommands::Disable(args) => {
                let client = make_client(&args)?;
                mqtt::deactivate_client(&client)?;
                format::ok_msg("MQTT client deactivated");
                Ok(())
            }
        }
    }
}

impl MqttConfigureCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);

        let mut params = serde_json::json!({
            "server": {
                "protocol": self.protocol,
                "host": self.broker,
                "port": self.broker_port,
            },
        });

        if let Some(client_id) = &self.client_id {
            params["clientId"] = serde_json::json!(client_id);
        }

        if let Some(mqtt_user) = &self.mqtt_user {
            params["username"] = serde_json::json!(mqtt_user);
            if let Some(mqtt_pass) = &self.mqtt_pass {
                params["password"] = serde_json::json!(mqtt_pass);
            }
        }

        let result = mqtt::configure_client(&client, &params)?;
        format::ok_msg("MQTT client configured");
        let _ = result;
        Ok(())
    }
}

impl MqttEventsCmd {
    fn run(self) -> anyhow::Result<()> {
        let client = make_client(&self.cam)?;

        // Read-only when no mutation flag is given.
        if self.add.is_empty() && self.remove.is_empty() && !self.clear {
            let result = mqtt::get_event_config(&client)?;
            format::ok(&result.get("data").unwrap_or(&result));
            return Ok(());
        }

        let current = mqtt::get_event_config(&client)?;

        // Preserve the full eventPublicationConfig object; only the filter list
        // is modified so unrelated fields (topicPrefix, appendEventTopic, ...)
        // are written back unchanged.
        let mut config = current
            .get("data")
            .and_then(|d| d.get("eventPublicationConfig"))
            .filter(|c| c.is_object())
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        apply_filter_changes(
            &mut config,
            &self.add,
            &self.remove,
            self.clear,
            self.qos,
            &self.retain,
        );

        mqtt::set_event_config(&client, &config)?;

        // Echo back the resulting config so callers can confirm the change.
        let updated = mqtt::get_event_config(&client)?;
        format::ok(&updated.get("data").unwrap_or(&updated));
        Ok(())
    }
}

/// Apply add/remove/clear changes to `config["eventFilterList"]` in place.
///
/// Order of operations: clear, then remove, then add. Adds are idempotent on
/// `topicFilter` so repeated runs do not create duplicates. Only the
/// `eventFilterList` key is touched; all other keys in `config` are preserved.
fn apply_filter_changes(
    config: &mut Value,
    add: &[String],
    remove: &[String],
    clear: bool,
    qos: u8,
    retain: &str,
) {
    let mut filters: Vec<Value> = config
        .get("eventFilterList")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if clear {
        filters.clear();
    }

    if !remove.is_empty() {
        filters.retain(|f| {
            f.get("topicFilter")
                .and_then(|t| t.as_str())
                .map(|t| !remove.iter().any(|r| r == t))
                .unwrap_or(true)
        });
    }

    for topic in add {
        let exists = filters.iter().any(|f| {
            f.get("topicFilter").and_then(|t| t.as_str()) == Some(topic.as_str())
        });
        if !exists {
            filters.push(serde_json::json!({
                "topicFilter": topic,
                "qos": qos,
                "retain": retain,
            }));
        }
    }

    config["eventFilterList"] = Value::Array(filters);
}

fn make_client(args: &MqttCameraArgs) -> anyhow::Result<VapixClient> {
    let (creds, resolved_host) = crate::cmd::resolve_cam(
        &args.host,
        args.user.as_deref(),
        args.pass.as_deref(),
        args.port,
        args.insecure,
    )?;
    Ok(crate::cmd::make_client(&resolved_host, creds, args.timeout))
}

#[cfg(test)]
mod tests {
    use super::apply_filter_changes;
    use serde_json::json;

    fn topics(config: &serde_json::Value) -> Vec<String> {
        config["eventFilterList"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["topicFilter"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn add_to_empty_config_creates_filter() {
        let mut config = json!({});
        apply_filter_changes(&mut config, &["oa/topic".into()], &[], false, 0, "none");
        assert_eq!(topics(&config), vec!["oa/topic"]);
        let f = &config["eventFilterList"][0];
        assert_eq!(f["qos"], 0);
        assert_eq!(f["retain"], "none");
    }

    #[test]
    fn add_preserves_other_config_fields() {
        let mut config = json!({
            "topicPrefix": "default",
            "appendEventTopic": true,
            "eventFilterList": [],
        });
        apply_filter_changes(&mut config, &["a".into()], &[], false, 1, "all");
        assert_eq!(config["topicPrefix"], "default");
        assert_eq!(config["appendEventTopic"], true);
        assert_eq!(config["eventFilterList"][0]["qos"], 1);
        assert_eq!(config["eventFilterList"][0]["retain"], "all");
    }

    #[test]
    fn add_is_idempotent_on_topic_filter() {
        let mut config = json!({
            "eventFilterList": [{"topicFilter": "a", "qos": 0, "retain": "none"}]
        });
        apply_filter_changes(&mut config, &["a".into()], &[], false, 2, "all");
        // No duplicate, original entry kept unchanged.
        assert_eq!(topics(&config), vec!["a"]);
        assert_eq!(config["eventFilterList"][0]["qos"], 0);
    }

    #[test]
    fn remove_drops_matching_topic() {
        let mut config = json!({
            "eventFilterList": [
                {"topicFilter": "a", "qos": 0, "retain": "none"},
                {"topicFilter": "b", "qos": 0, "retain": "none"}
            ]
        });
        apply_filter_changes(&mut config, &[], &["a".into()], false, 0, "none");
        assert_eq!(topics(&config), vec!["b"]);
    }

    #[test]
    fn clear_then_add_replaces_list() {
        let mut config = json!({
            "eventFilterList": [{"topicFilter": "old", "qos": 0, "retain": "none"}]
        });
        apply_filter_changes(&mut config, &["new".into()], &[], true, 0, "none");
        assert_eq!(topics(&config), vec!["new"]);
    }

    #[test]
    fn clear_only_empties_list() {
        let mut config = json!({
            "eventFilterList": [{"topicFilter": "old", "qos": 0, "retain": "none"}]
        });
        apply_filter_changes(&mut config, &[], &[], true, 0, "none");
        assert!(topics(&config).is_empty());
    }

    #[test]
    fn remove_then_add_in_single_call() {
        let mut config = json!({
            "eventFilterList": [{"topicFilter": "a", "qos": 0, "retain": "none"}]
        });
        apply_filter_changes(&mut config, &["b".into()], &["a".into()], false, 0, "none");
        assert_eq!(topics(&config), vec!["b"]);
    }
}
