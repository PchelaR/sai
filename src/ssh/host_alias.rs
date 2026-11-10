use std::fs;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::files;
use crate::vps::{HOST_ALIAS, SSH_PORT, USER};

const BEGIN: &str = "# BEGIN sai";
const END: &str = "# END sai";
const MULTIPLEXING: &str =
    "    ControlMaster auto\n    ControlPath ~/.ssh/sai-%C\n    ControlPersist 10m\n";

pub fn install(config: &Config) -> Result<()> {
    let dir = super::dir()?;
    let path = dir.join("config");
    let existing = files::read_or_empty(&path)?;
    let content = format!(
        "{}\n{}",
        host_block(config)?,
        without_block(&existing).trim_start()
    );
    if content == existing {
        return Ok(());
    }
    fs::create_dir_all(&dir)?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn host_block(config: &Config) -> Result<String> {
    let multiplexing = if cfg!(windows) { "" } else { MULTIPLEXING };
    Ok(format!(
        "{BEGIN}\nHost {HOST_ALIAS}\n    HostName {}\n    Port {SSH_PORT}\n    User {USER}\n    IdentityFile \"{}\"\n    IdentitiesOnly yes\n    ServerAliveInterval 30\n{multiplexing}{END}\n",
        config.ip,
        config.private_key()?.display()
    ))
}

fn without_block(content: &str) -> String {
    let mut inside = false;
    content
        .lines()
        .filter(|line| match line.trim() {
            BEGIN => {
                inside = true;
                false
            }
            END => {
                inside = false;
                false
            }
            _ => !inside,
        })
        .map(|line| format!("{line}\n"))
        .collect()
}
