use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use russh::{ChannelMsg, client};

use super::host_key::HostKeyPolicy;
use crate::quote;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const PASSWORD_PORT: u16 = 22;
const STDERR_TAIL_LINES: usize = 30;

pub type Env<'a> = [(&'a str, String)];

pub struct Session {
    handle: client::Handle<HostKeyPolicy>,
    sudo_password: Option<String>,
}

impl Session {
    pub async fn connect(ip: &str, user: &str, password: &str) -> Result<Self> {
        let config = Arc::new(client::Config {
            keepalive_interval: Some(KEEPALIVE_INTERVAL),
            ..Default::default()
        });
        let connecting = client::connect(config, (ip, PASSWORD_PORT), HostKeyPolicy::new(ip));
        let mut handle = tokio::time::timeout(CONNECT_TIMEOUT, connecting)
            .await
            .with_context(|| format!("timed out connecting to {ip}:{PASSWORD_PORT}"))?
            .with_context(|| format!("could not connect to {ip}:{PASSWORD_PORT}"))?;
        let auth = handle.authenticate_password(user, password).await?;
        ensure!(auth.success(), "wrong user or password for {user}@{ip}");
        Ok(Self {
            handle,
            sudo_password: (user != "root").then(|| password.to_owned()),
        })
    }

    pub async fn run_script(&self, script: &str, env: &Env<'_>) -> Result<()> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, self.command(script, env)).await?;
        channel.data(self.stdin().as_bytes()).await?;
        channel.eof().await?;

        let mut stderr = Vec::new();
        let mut exit_status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                _ => {}
            }
        }

        match exit_status {
            Some(0) => Ok(()),
            Some(code) => bail!("exit code {code}\n{}", tail(&stderr)),
            None => bail!("connection closed unexpectedly"),
        }
    }

    fn command(&self, script: &str, env: &Env<'_>) -> String {
        let encoded = BASE64_STANDARD.encode(script);
        let sudo = if self.sudo_password.is_some() {
            "sudo -S -p '' "
        } else {
            ""
        };
        let env: String = env
            .iter()
            .map(|(name, value)| format!(" {name}={}", quote::shell(value)))
            .collect();
        format!(
            r#"f=$(mktemp) && echo {encoded} | base64 -d > "$f" && {sudo}env{env} bash "$f"; s=$?; rm -f "$f"; exit $s"#
        )
    }

    fn stdin(&self) -> String {
        self.sudo_password
            .as_ref()
            .map(|password| format!("{password}\n"))
            .unwrap_or_default()
    }
}

fn tail(output: &[u8]) -> String {
    let output = String::from_utf8_lossy(output);
    let lines: Vec<&str> = output.trim().lines().collect();
    lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n")
}
