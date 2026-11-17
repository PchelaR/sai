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
const ATTEMPTS: u32 = 3;
const UNREACHABLE: &[&str] = &[
    "connection refused",
    "connection timed out",
    "operation timed out",
    "could not resolve hostname",
    "network is unreachable",
    "no route to host",
    "connection reset",
    "connection closed",
    "kex_exchange_identification",
];
const LOCALE_FIX: &str = "for name in LANG LC_ALL LC_CTYPE; do if [ \"${!name:-}\" = UTF-8 ]; then export \"$name=C.UTF-8\"; fi; done\n";

pub fn run(script: &str, args: &[&str]) -> Result<()> {
    run_named(first_line(script), script, args)
}

pub fn run_all(name: &str, steps: &[(&str, Vec<String>)]) -> Result<()> {
    run_named(name, &combine(steps), &[])
}

pub fn output_all(name: &str, steps: &[(&str, Vec<String>)]) -> Result<String> {
    output_named(name, &combine(steps), &[])
}

fn combine(steps: &[(&str, Vec<String>)]) -> String {
    steps
        .iter()
        .map(|(script, args)| {
            let positional: String = args
                .iter()
                .map(|arg| format!(" {}", quote::shell(arg)))
                .collect();
            format!("(\nset --{positional}\n{script}\n) </dev/null\n")
        })
        .collect()
}

fn run_named(name: &str, script: &str, args: &[&str]) -> Result<()> {
    let mut attempt = 1;
    loop {
        let started = Instant::now();
        let (status, stderr, printed) = run_once(name, script, args)?;
        if !printed && attempt < ATTEMPTS && unreachable(status, &stderr) {
            wait_before_retry(attempt);
            attempt += 1;
            continue;
        }
        return check(name, started, status, &stderr);
    }
}

fn run_once(name: &str, script: &str, args: &[&str]) -> Result<(ExitStatus, String, bool)> {
    let mut child = spawn(name, script, args)?;
    let stderr = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            stderr.read_to_string(&mut text).ok();
            text
        })
    });
    let mut printed = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            output::info(&line?);
            printed = true;
        }
    }
    let status = child.wait()?;
    let stderr = stderr
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    Ok((status, stderr, printed))
}

fn unreachable(status: ExitStatus, stderr: &str) -> bool {
    let stderr = stderr.to_lowercase();
    status.code() == Some(SSH_CONNECTION_FAILED)
        && UNREACHABLE.iter().any(|reason| stderr.contains(reason))
}

fn wait_before_retry(attempt: u32) {
    logs::line(&format!(
        "the VPS is unreachable, attempt {attempt} of {ATTEMPTS}"
    ));
    output::info(&format!(
        "Could not reach the VPS, trying again in {} s ({attempt} of {})",
        RETRY_DELAY.as_secs(),
        ATTEMPTS - 1
    ));
    thread::sleep(RETRY_DELAY);
}

pub fn probe(script: &str, args: &[&str]) -> Option<String> {
    let output = spawn(first_line(script), script, args)
        .ok()?
        .wait_with_output()
        .ok()?;
    output
        .status
        .success()
        .then(|| output::printable(&String::from_utf8_lossy(&output.stdout)))
}

pub fn output(script: &str, args: &[&str]) -> Result<String> {
    output_named(first_line(script), script, args)
}

fn output_named(name: &str, script: &str, args: &[&str]) -> Result<String> {
    let mut attempt = 1;
    let (started, output) = loop {
        let started = Instant::now();
        let output = spawn(name, script, args)?.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.stdout.is_empty() && attempt < ATTEMPTS && unreachable(output.status, &stderr) {
            wait_before_retry(attempt);
            attempt += 1;
            continue;
        }
        break (started, output);
    };
    check(
        name,
        started,
        output.status,
        &String::from_utf8_lossy(&output.stderr),
    )?;
    Ok(output::printable(&String::from_utf8_lossy(&output.stdout)))
}

pub fn interactive(command: &str) -> Result<()> {
    console::ensure_supported()?;
    logs::line(&format!("ssh -t {HOST_ALIAS} {command}"));
    let remote = format!(
        "bash <(echo {} | base64 -d)",
        BASE64_STANDARD.encode(format!("{LOCALE_FIX}{command}"))
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

fn spawn(name: &str, script: &str, args: &[&str]) -> Result<Child> {
    logs::line(&format!(
        "ssh {HOST_ALIAS} script {name}, args: {}",
        logs::describe(args)
    ));
    let mut child = Command::new(ssh::program("ssh"))
        .arg("-T")
        .args(["-o", "ConnectTimeout=10"])
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

fn check(name: &str, started: Instant, status: ExitStatus, stderr: &str) -> Result<()> {
    let stderr = output::printable(stderr.trim());
    let stderr = stderr.as_str();
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
            "a step on the VPS (`{name}`) stopped with {status} without a message, details are in `sai logs`"
        ),
        _ => bail!("{stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::stdin_script;

    #[cfg(unix)]
    #[test]
    fn retries_only_when_the_vps_is_unreachable() {
        use std::os::unix::process::ExitStatusExt;
        let failed = std::process::ExitStatus::from_raw(255 << 8);
        let script_failed = std::process::ExitStatus::from_raw(1 << 8);
        assert!(super::unreachable(
            failed,
            "ssh: connect to host 1.2.3.4 port 22: Connection refused"
        ));
        assert!(super::unreachable(
            failed,
            "ssh: Could not resolve hostname x"
        ));
        assert!(!super::unreachable(
            failed,
            "Permission denied (publickey)."
        ));
        assert!(!super::unreachable(script_failed, "Connection refused"));
    }

    #[cfg(unix)]
    #[test]
    fn runs_several_steps_in_one_connection() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        use super::combine;

        let steps = [
            ("echo first; exit 0\necho skipped", Vec::new()),
            ("cat; echo \"$1\"", vec!["two words".to_owned()]),
            ("false", Vec::new()),
            ("echo never", Vec::new()),
        ];
        let mut child = Command::new("bash")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin_script(&combine(&steps), &[]).as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "first\ntwo words\n"
        );
    }

    #[test]
    fn passes_arguments_through_stdin() {
        assert_eq!(
            stdin_script("echo \"$1\"\n", &["W:\\Dev\\it's"]),
            "set -- 'W:\\Dev\\it'\\''s'\nset -euo pipefail\necho \"$1\"\n"
        );
    }
}
