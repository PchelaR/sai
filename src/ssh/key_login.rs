use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use crate::config::Config;
use crate::output;
use crate::vps::{SETUP_MARKER, SSH_PORT, USER};

pub fn verify(config: &Config) -> Result<()> {
    run(config, "true")
}

pub fn is_setup_complete(config: &Config) -> bool {
    run(config, &format!("test -f {SETUP_MARKER}")).is_ok()
}

fn run(config: &Config, command: &str) -> Result<()> {
    match attempt(config, command) {
        Err(error) if cfg!(windows) && super::is_unprotected_key(&error.to_string()) => {
            protect(&config.private_key()?)?;
            attempt(config, command)
        }
        result => result,
    }
}

fn protect(key: &Path) -> Result<()> {
    let user = env::var("USERNAME").context("USERNAME is not set")?;
    output::info(&format!(
        "Restricting access to {} so that ssh accepts it",
        key.display()
    ));
    let status = Command::new("icacls")
        .arg(key)
        .args(["/inheritance:r", "/grant:r"])
        .arg(format!("{user}:F"))
        .stdout(Stdio::null())
        .status()
        .context("failed to run icacls")?;
    ensure!(
        status.success(),
        "failed to restrict access to {}",
        key.display()
    );
    Ok(())
}

fn attempt(config: &Config, command: &str) -> Result<()> {
    let output = Command::new(super::program("ssh"))
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
