use clap::{Args, Subcommand, ValueEnum};

use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::users;

#[derive(Args)]
pub struct UserCmd {
    #[command(subcommand)]
    pub command: UserCommands,
}

#[derive(Subcommand)]
pub enum UserCommands {
    /// List user accounts and groups
    List(UserListCmd),
    /// Add a new user account
    Add(UserAddCmd),
    /// Update a user account (change password)
    Update(UserUpdateCmd),
    /// Remove a user account
    Remove(UserRemoveCmd),
}

#[derive(Clone, ValueEnum)]
pub enum Role {
    /// Admin role (admin:operator:viewer)
    Admin,
    /// Operator role (operator:viewer)
    Operator,
    /// Viewer role (viewer only)
    Viewer,
}

impl Role {
    fn to_sgrp(&self, ptz: bool) -> String {
        let base = match self {
            Role::Admin => "admin:operator:viewer",
            Role::Operator => "operator:viewer",
            Role::Viewer => "viewer",
        };
        if ptz {
            format!("{}:ptz", base)
        } else {
            base.to_string()
        }
    }
}

#[derive(Args)]
pub struct UserListCmd {
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
    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct UserAddCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Account name to create (1-14 chars, a-z A-Z 0-9)
    #[arg(long)]
    pub name: String,
    /// Password for the new account
    #[arg(long)]
    pub pwd: String,
    /// Role for the account
    #[arg(long, default_value = "viewer")]
    pub role: Role,
    /// Grant PTZ control
    #[arg(long)]
    pub ptz: bool,
    /// Account description
    #[arg(long, default_value = "")]
    pub comment: String,
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
pub struct UserUpdateCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Account name to update
    #[arg(long)]
    pub name: String,
    /// New password
    #[arg(long)]
    pub pwd: String,
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
pub struct UserRemoveCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Account name to remove
    #[arg(long)]
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

impl UserCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            UserCommands::List(cmd) => cmd.run(),
            UserCommands::Add(cmd) => cmd.run(),
            UserCommands::Update(cmd) => cmd.run(),
            UserCommands::Remove(cmd) => cmd.run(),
        }
    }
}

impl UserListCmd {
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
        let text = users::list(&client)?;

        if self.plain {
            print!("{}", text);
        } else {
            let mut map = serde_json::Map::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let users_str = v.trim_matches('"');
                    let user_list: Vec<serde_json::Value> = users_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect();
                    map.insert(k.to_string(), serde_json::Value::Array(user_list));
                }
            }
            crate::output::format::ok(&map);
        }

        Ok(())
    }
}

impl UserAddCmd {
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
        let sgrp = self.role.to_sgrp(self.ptz);
        let result = users::add(&client, &self.name, &self.pwd, &sgrp, &self.comment)?;
        crate::output::format::ok_msg(result.trim());
        Ok(())
    }
}

impl UserUpdateCmd {
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
        let result = users::update(&client, &self.name, &self.pwd)?;
        crate::output::format::ok_msg(result.trim());
        Ok(())
    }
}

impl UserRemoveCmd {
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
        let result = users::remove(&client, &self.name)?;
        crate::output::format::ok_msg(result.trim());
        Ok(())
    }
}
