use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::vps::HOST_ALIAS;
use crate::{console, logs, output, quote, ssh};

const PRELUDE: &str = "set -euo pipefail\n";
const SSH_MISSING: &str = "failed to run ssh, make sure OpenSSH client is installed";
const SSH_CONNECTION_FAILED: i32 = 255;
const LIVE_SESSION: Duration = Duration::from_secs(10);
const RETRY_DELAY: Duration = Duration::from_secs(5);

pub fn run(script: &str, args: &[&str]) -> Result<()> {
    let started = Instant::now();
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
    check(script, started, status, &stderr)
}

pub fn output(script: &str, args: &[&str]) -> Result<String> {
    let started = Instant::now();
    let output = spawn(script, args)?.wait_with_output()?;
    check(
        script,
        started,
        output.status,
        &String::from_utf8_lossy(&output.stderr),
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn interactive(command: &str) -> Result<()> {
    console::ensure_supported()?;
    logs::line(&format!("ssh -t {HOST_ALIAS} {command}"));
    let remote = format!(
        "bash <(echo {} | base64 -d)",
        BASE64_STANDARD.encode(command)
    );
    let mut reconnecting = false;
    loop {
        let started = Instant::now();
        let status = {
            let _session = console::begin_session();
            Command::new(ssh::program("ssh"))
                .args(["-t", "-q"])
                .args(["-o", "ConnectTimeout=10"])
                .args(["-o", "ServerAliveInterval=10"])
                .args(["-o", "ServerAliveCountMax=3"])
                .arg(HOST_ALIAS)
                .arg(&remote)
                .status()
                .context(SSH_MISSING)?
        };
        logs::line(&format!("remote session ended with {status}"));
        if status.success() {
            return Ok(());
        }
        let connection_lost = status.code() == Some(SSH_CONNECTION_FAILED)
            && (reconnecting || started.elapsed() >= LIVE_SESSION);
        ensure!(connection_lost, "remote session ended with {status}");
        if !reconnecting {
            output::info("");
            output::info(
                "Connection to the VPS was lost. The agent keeps working there, reconnecting... Press Ctrl+C to stop",
            );
        }
        reconnecting = true;
        thread::sleep(RETRY_DELAY);
    }
}

fn spawn(script: &str, args: &[&str]) -> Result<Child> {
    logs::line(&format!(
        "ssh {HOST_ALIAS} script {}, args: {}",
        first_line(script),
        logs::describe(args)
    ));
    let mut child = Command::new(ssh::program("ssh"))
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

fn first_line(script: &str) -> &str {
    script
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
}

fn check(script: &str, started: Instant, status: ExitStatus, stderr: &str) -> Result<()> {
    let stderr = stderr.trim();
    logs::line(&format!(
        "finished with {status} in {:.1}s{}{stderr}",
        started.elapsed().as_secs_f64(),
        if stderr.is_empty() { "" } else { ", stderr:\n" }
    ));
    match status.code() {
        Some(0) => Ok(()),
        Some(SSH_CONNECTION_FAILED) if ssh::is_unprotected_key(stderr) => {
            bail!(
                "ssh refuses the key because others can read it, run `sai doctor` to fix that\n{stderr}"
            )
        }
        Some(SSH_CONNECTION_FAILED) => {
            bail!(
                "could not connect to the VPS. Check your internet connection and that the VPS is running, if it was never set up run `sai setup`\n{stderr}"
            )
        }
        _ if stderr.is_empty() => bail!(
            "a step on the VPS (`{}`) stopped with {status} without a message, details are in `sai logs`",
            first_line(script)
        ),
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
