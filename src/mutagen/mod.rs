mod install;
mod session;

use std::{ffi::OsStr, process::Command};

use anyhow::{Context, Result, ensure};

use session::Session;

use crate::project::Project;
use crate::secrets;
use crate::vps::HOST_ALIAS;
use crate::{files, ignores, output};

const IGNORE_FILE: &str = ".saiignore";
const LIST_TEMPLATE: &str = "{{ json . }}";
const MIRROR: &str = "one-way-replica";
const TWO_WAY: &str = "two-way-safe";

pub fn ensure_installed() -> Result<()> {
    install::binary().map(|_| ())
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

pub fn sync(project: &Project) -> Result<()> {
    let id = project.id();
    let ignores = ignores(project)?;
    let Some(session) = session(&id)? else {
        output::step("Uploading project");
        mirror(project, &ignores)?;
        output::step("Starting sync");
        return create(project, TWO_WAY, &ignores);
    };
    if session.paused {
        run(&["sync", "resume", &id])?;
    }
    if session.ignore.paths != ignores {
        output::step("Applying new ignore rules");
        resync(project, &ignores)?;
    }
    Ok(())
}

pub fn keep_local(project: &Project) -> Result<()> {
    ensure!(
        session(&project.id())?.is_some(),
        "{} is not attached, run `sai chat <dir>` first",
        project.name
    );
    resync(project, &ignores(project)?)
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

fn resync(project: &Project, ignores: &[String]) -> Result<()> {
    let id = project.id();
    run(&["sync", "flush", &id]).context("failed to finish the current sync, try again later")?;
    terminate(&id)?;
    mirror(project, ignores)?;
    create(project, TWO_WAY, ignores)
}

fn mirror(project: &Project, ignores: &[String]) -> Result<()> {
    let id = project.id();
    create(project, MIRROR, ignores)?;
    run(&["sync", "flush", &id])?;
    terminate(&id)
}

fn create(project: &Project, mode: &str, ignores: &[String]) -> Result<()> {
    let mut args = vec![
        "sync".to_owned(),
        "create".to_owned(),
        format!("--name={}", project.id()),
        format!("--sync-mode={mode}"),
        "--ignore-vcs".to_owned(),
    ];
    args.extend(ignores.iter().map(|pattern| format!("--ignore={pattern}")));
    args.push(project.local_path.display().to_string());
    args.push(format!("{HOST_ALIAS}:{}", project.remote_path()));
    run(&args).map(|_| ())
}

fn ignores(project: &Project) -> Result<Vec<String>> {
    let path = project.local_path.join(IGNORE_FILE);
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
    let output = Command::new(install::binary()?)
        .env("MUTAGEN_DATA_DIRECTORY", install::data_directory()?)
        .args(args)
        .output()
        .context("failed to run mutagen")?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
