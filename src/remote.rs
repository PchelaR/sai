use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};

use crate::quote;
use crate::vps::HOST_ALIAS;

const PRELUDE: &str = "set -euo pipefail\n";
const SSH_MISSING: &str = "failed to run ssh, make sure OpenSSH client is installed";
const SSH_CONNECTION_FAILED: i32 = 255;

pub fn run(script: &str, args: &[&str]) -> Result<()> {
    execute(script, args, Stdio::inherit()).map(|_| ())
}

pub fn output(script: &str, args: &[&str]) -> Result<String> {
    execute(script, args, Stdio::piped())
}

pub fn interactive(command: &str) -> Result<()> {
    let status = Command::new("ssh")
        .arg("-t")
        .arg(HOST_ALIAS)
        .arg(command)
        .status()
        .context(SSH_MISSING)?;
    ensure!(status.success(), "remote session ended with {status}");
    Ok(())
}

fn execute(script: &str, args: &[&str], stdout: Stdio) -> Result<String> {
    let command: String = args
        .iter()
        .map(|arg| format!(" {}", quote::shell(arg)))
        .collect();
    let mut child = Command::new("ssh")
        .arg("-T")
        .arg(HOST_ALIAS)
        .arg(format!("bash -s --{command}"))
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .context(SSH_MISSING)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(format!("{PRELUDE}{script}").as_bytes())?;
    }
    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        Some(SSH_CONNECTION_FAILED) => {
            bail!("could not connect to the VPS, run `sai setup` first\n{stderr}")
        }
        _ => bail!("{stderr}"),
    }
}
