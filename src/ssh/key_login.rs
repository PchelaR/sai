use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use crate::config::Config;
use crate::vps::{SETUP_MARKER, SSH_PORT, USER};

pub fn verify(config: &Config) -> Result<()> {
    run(config, "true")
}

pub fn is_setup_complete(config: &Config) -> bool {
    run(config, &format!("test -f {SETUP_MARKER}")).is_ok()
}

fn run(config: &Config, command: &str) -> Result<()> {
    let output = Command::new("ssh")
        .args(["-F", "none"])
        .arg("-i")
        .arg(config.private_key()?)
        .arg("-p")
        .arg(SSH_PORT.to_string())
        .args(["-o", "ConnectTimeout=10"])
        .args(["-o", "StrictHostKeyChecking=yes"])
        .args(["-o", "IdentitiesOnly=yes"])
        .args(["-o", "PasswordAuthentication=no"])
        .args(["-o", "KbdInteractiveAuthentication=no"])
        .arg(format!("{USER}@{}", config.ip))
        .arg(command)
        .stdin(Stdio::null())
        .output()
        .context("failed to run ssh, make sure OpenSSH client is installed")?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}
