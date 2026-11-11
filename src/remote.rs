use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::output;
use crate::quote;
use crate::vps::HOST_ALIAS;

const PRELUDE: &str = "set -euo pipefail\n";
const SSH_MISSING: &str = "failed to run ssh, make sure OpenSSH client is installed";
const SSH_CONNECTION_FAILED: i32 = 255;

pub fn run(script: &str, args: &[&str]) -> Result<()> {
    let mut child = spawn(script, args)?;
    let stderr = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            stderr.read_to_string(&mut text).ok();
            text
        })
    });
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            output::info(&line?);
        }
    }
    let status = child.wait()?;
    let stderr = stderr
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    check(status, &stderr)
}

pub fn output(script: &str, args: &[&str]) -> Result<String> {
    let output = spawn(script, args)?.wait_with_output()?;
    check(output.status, &String::from_utf8_lossy(&output.stderr))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn interactive(command: &str) -> Result<()> {
    let status = Command::new("ssh")
        .args(["-t", "-q"])
        .arg(HOST_ALIAS)
        .arg(format!(
            "bash <(echo {} | base64 -d)",
            BASE64_STANDARD.encode(command)
        ))
        .status()
        .context(SSH_MISSING)?;
    ensure!(status.success(), "remote session ended with {status}");
    Ok(())
}

fn spawn(script: &str, args: &[&str]) -> Result<Child> {
    let mut child = Command::new("ssh")
        .arg("-T")
        .arg(HOST_ALIAS)
        .arg("bash -s")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(SSH_MISSING)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_script(script, args).as_bytes())?;
    }
    Ok(child)
}

fn stdin_script(script: &str, args: &[&str]) -> String {
    let positional: String = args
        .iter()
        .map(|arg| format!(" {}", quote::shell(arg)))
        .collect();
    format!("set --{positional}\n{PRELUDE}{script}")
}

fn check(status: ExitStatus, stderr: &str) -> Result<()> {
    let stderr = stderr.trim();
    match status.code() {
        Some(0) => Ok(()),
        Some(SSH_CONNECTION_FAILED) => {
            bail!("could not connect to the VPS, run `sai setup` first\n{stderr}")
        }
        _ => bail!("{stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::stdin_script;

    #[test]
    fn passes_arguments_through_stdin() {
        assert_eq!(
            stdin_script("echo \"$1\"\n", &["W:\\Dev\\it's"]),
            "set -- 'W:\\Dev\\it'\\''s'\nset -euo pipefail\necho \"$1\"\n"
        );
    }
}
