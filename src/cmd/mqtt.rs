use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::mqtt;

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
    /// Show event publication configuration
    Events(MqttCameraArgs),
    /// Enable (activate) MQTT client
    Enable(MqttCameraArgs),
    /// Disable (deactivate) MQTT client
    Disable(MqttCameraArgs),
}

#[derive(Args)]
pub struct MqttCameraArgs {
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
            MqttCommands::Events(args) => {
                let client = make_client(&args)?;
                let result = mqtt::get_event_config(&client)?;
                format::ok(&result.get("data").unwrap_or(&result));
                Ok(())
            }
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

fn make_client(args: &MqttCameraArgs) -> anyhow::Result<VapixClient> {
    let (creds, resolved_host) = resolve(
        &args.host,
        args.user.as_deref(),
        args.pass.as_deref(),
        args.port,
        args.insecure,
    )?;
    let timeout = args.timeout.unwrap_or(creds.timeout);
    Ok(VapixClient::new(&resolved_host, creds.port, creds, timeout))
}
