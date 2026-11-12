use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use russh::keys::{self, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, client};

use super::host_key::HostKeyPolicy;
use crate::config::Config;
use crate::{logs, output, quote};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const STDERR_TAIL_LINES: usize = 30;
const PROGRESS_INDENT: &str = "  ";

pub type Env<'a> = [(&'a str, String)];

pub enum Auth {
    Password(String),
    Key,
}

enum Sudo {
    None,
    Password(String),
    NonInteractive,
}

pub struct Session {
    handle: client::Handle<HostKeyPolicy>,
    sudo: Sudo,
}

impl Session {
    pub async fn connect(config: &Config, port: u16, user: &str, auth: Auth) -> Result<Self> {
        let ip = config.ip.as_str();
        let client_config = Arc::new(client::Config {
            keepalive_interval: Some(KEEPALIVE_INTERVAL),
            ..Default::default()
        });
        let policy = HostKeyPolicy::new(ip, port, config.port());
        let connecting = client::connect(client_config, (ip, port), policy);
        let mut handle = tokio::time::timeout(CONNECT_TIMEOUT, connecting)
            .await
            .with_context(|| format!("timed out connecting to {ip}:{port}"))?
            .with_context(|| format!("could not connect to {ip}:{port}"))?;
        let accepted = match &auth {
            Auth::Password(password) => handle.authenticate_password(user, password).await?,
            Auth::Key => {
                let path = config.private_key()?;
                let key = keys::load_secret_key(&path, None).with_context(|| {
                    format!(
                        "cannot use {} without a passphrase, enter the password instead",
                        path.display()
                    )
                })?;
                let hash = handle.best_supported_rsa_hash().await?.flatten();
                handle
                    .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                    .await?
            }
        };
        ensure!(
            accepted.success(),
            "{user}@{ip} rejected the login, check the user and the password or key"
        );
        let sudo = match auth {
            _ if user == "root" => Sudo::None,
            Auth::Password(password) => Sudo::Password(password),
            Auth::Key => Sudo::NonInteractive,
        };
        Ok(Self { handle, sudo })
    }

    pub async fn run_script(&self, script: &str, env: &Env<'_>) -> Result<()> {
        self.execute(script, env, true).await.map(|_| ())
    }

    pub async fn output(&self, script: &str, env: &Env<'_>) -> Result<String> {
        self.execute(script, env, false).await
    }

    async fn execute(&self, script: &str, env: &Env<'_>, stream: bool) -> Result<String> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, self.command(script, env)).await?;
        channel.data(self.stdin().as_bytes()).await?;
        channel.eof().await?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    stdout.extend_from_slice(&data);
                    if stream {
                        while let Some(end) = stdout.iter().position(|byte| *byte == b'\n') {
                            let line: Vec<u8> = stdout.drain(..=end).collect();
                            let line = String::from_utf8_lossy(&line);
                            if line.starts_with(PROGRESS_INDENT) {
                                output::info(line.trim_end());
                            } else {
                                logs::line(line.trim_end());
                            }
                        }
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                _ => {}
            }
        }

        match exit_status {
            Some(0) => Ok(String::from_utf8_lossy(&stdout).into_owned()),
            Some(code) => bail!("exit code {code}\n{}", tail(&stderr)),
            None => bail!("connection closed unexpectedly"),
        }
    }

    fn command(&self, script: &str, env: &Env<'_>) -> String {
        let encoded = BASE64_STANDARD.encode(script);
        let sudo = match self.sudo {
            Sudo::None => "",
            Sudo::Password(_) => "sudo -S -p '' ",
            Sudo::NonInteractive => "sudo -n ",
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
        match &self.sudo {
            Sudo::Password(password) => format!("{password}\n"),
            Sudo::None | Sudo::NonInteractive => String::new(),
        }
    }
}

fn tail(output: &[u8]) -> String {
    let output = String::from_utf8_lossy(output);
    let lines: Vec<&str> = output.trim().lines().collect();
    lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n")
}
