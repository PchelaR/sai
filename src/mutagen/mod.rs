mod install;
mod session;

use std::{ffi::OsStr, process::Command};

use anyhow::{Context, Result, ensure};

use session::Session;

use crate::project::SyncTarget;
use crate::vps::HOST_ALIAS;
use crate::{files, ignores, logs, output};
use crate::{secrets, ssh};

const IGNORE_FILE: &str = ".saiignore";
const LIST_TEMPLATE: &str = "{{ json . }}";
const MIRROR: &str = "one-way-replica";
const TWO_WAY: &str = "two-way-safe";

pub fn ensure_installed() -> Result<()> {
    install::binary().map(|_| ())
}

pub fn describe() -> Result<String> {
    run(&["sync", "list"])
}

pub fn terminate_all() -> Result<()> {
    run(&["sync", "terminate", "--all"]).map(|_| ())
}

pub fn session(id: &str) -> Result<Option<Session>> {
    let Ok(output) = run(&["sync", "list", id, "--template", LIST_TEMPLATE]) else {
        return Ok(None);
    };
    Ok(parse_sessions(&output)?.into_iter().next())
}

pub fn sessions() -> Result<Vec<Session>> {
    parse_sessions(&run(&["sync", "list", "--template", LIST_TEMPLATE])?)
}

fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    let sessions: Option<Vec<Session>> =
        serde_json::from_str(output).context("unexpected output from `mutagen sync list`")?;
    Ok(sessions.unwrap_or_default())
}

pub fn sync(target: &SyncTarget) -> Result<()> {
    let ignores = ignores(target)?;
    let Some(session) = session(&target.id)? else {
        output::step(&format!("Uploading {}", target.local.display()));
        mirror(target, &ignores)?;
        return create(target, TWO_WAY, &ignores);
    };
    if session.paused {
        run(&["sync", "resume", &target.id])?;
    }
    if session.ignore.paths != ignores {
        output::step(&format!(
            "Applying new ignore rules to {}",
            target.local.display()
        ));
        resync(target, &ignores)?;
    }
    Ok(())
}

pub fn keep_local(target: &SyncTarget) -> Result<()> {
    ensure!(
        session(&target.id)?.is_some(),
        "{} is not syncing, run `sai chat` first",
        target.local.display()
    );
    resync(target, &ignores(target)?)
}

pub fn flush(id: &str) -> Result<()> {
    run(&["sync", "flush", id])
        .map(|_| ())
        .context("failed to finish the current sync, check the connection and try again")
}

pub fn report_issues(id: &str) -> Result<()> {
    if let Some(session) = session(id)? {
        for issue in session.issues() {
            output::warning(&issue);
        }
    }
    Ok(())
}

pub fn pause(id: &str) -> Result<()> {
    run(&["sync", "pause", id]).map(|_| ())
}

pub fn terminate(id: &str) -> Result<()> {
    run(&["sync", "terminate", id]).map(|_| ())
}

fn resync(target: &SyncTarget, ignores: &[String]) -> Result<()> {
    flush(&target.id)?;
    terminate(&target.id)?;
    mirror(target, ignores)?;
    create(target, TWO_WAY, ignores)
}

fn mirror(target: &SyncTarget, ignores: &[String]) -> Result<()> {
    create(target, MIRROR, ignores)?;
    flush(&target.id)?;
    terminate(&target.id)
}

fn create(target: &SyncTarget, mode: &str, ignores: &[String]) -> Result<()> {
    let mut args = vec![
        "sync".to_owned(),
        "create".to_owned(),
        format!("--name={}", target.id),
        format!("--sync-mode={mode}"),
        "--ignore-vcs".to_owned(),
    ];
    args.extend(ignores.iter().map(|pattern| format!("--ignore={pattern}")));
    args.push(target.local.display().to_string());
    args.push(format!("{HOST_ALIAS}:{}", target.remote));
    run(&args).map(|_| ())
}

fn ignores(target: &SyncTarget) -> Result<Vec<String>> {
    let path = target.local.join(IGNORE_FILE);
    let custom = files::read_or_empty(&path)?;
    Ok(ignores::BUILD
        .iter()
        .chain(secrets::PATTERNS)
        .map(|pattern| pattern.to_string())
        .chain(
            custom
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned),
        )
        .collect())
}

fn run<S: AsRef<OsStr>>(args: &[S]) -> Result<String> {
    let mut command = Command::new(install::binary()?);
    command.env("MUTAGEN_DATA_DIRECTORY", install::data_directory()?);
    if let Some(dir) = ssh::windows_openssh() {
        command.env("MUTAGEN_SSH_PATH", dir);
    }
    let output = command
        .args(args)
        .output()
        .context("failed to run mutagen")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        logs::line(&format!(
            "mutagen {} failed with {}: {}",
            logs::describe(args),
            output.status,
            stderr.trim()
        ));
    }
    ensure!(output.status.success(), "{}", stderr.trim());
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
