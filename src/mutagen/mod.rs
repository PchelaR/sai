mod install;
mod session;

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

use session::Session;

use crate::project::SyncTarget;
use crate::vps::HOST_ALIAS;
use crate::{ignores, logs, output};
use crate::{secrets, ssh};

const LIST_TEMPLATE: &str = "{{ json . }}";
const MIRROR: &str = "one-way-replica";
const TWO_WAY: &str = "two-way-resolved";
const LOCAL_WINS: &str = "two-way-safe";
const GIT_IGNORES: &[&str] = &[
    "/config",
    "/hooks/",
    "/logs/",
    "/FETCH_HEAD",
    "/lfs/",
    "*.lock",
];
const PROGRESS_INTERVAL: Duration = Duration::from_secs(2);
const FLUSH_BEFORE_PAUSE: Duration = Duration::from_secs(15);
const SAI_PREFIX: &str = "sai-";
const REPEAT_AFTER: Duration = Duration::from_secs(20);

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

pub fn session_names() -> Result<Vec<String>> {
    Ok(sessions()?
        .into_iter()
        .map(|session| session.name)
        .collect())
}

fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    let sessions: Option<Vec<Session>> =
        serde_json::from_str(output).context("unexpected output from `mutagen sync list`")?;
    Ok(sessions.unwrap_or_default())
}

pub fn needs_upload(target: &SyncTarget) -> Result<bool> {
    Ok(session(&target.id)?.is_none_or(|session| session.mode == MIRROR))
}

pub fn sync(target: &SyncTarget) -> Result<()> {
    let ignores = ignores(target)?;
    let Some(session) = session(&target.id)? else {
        output::step(&format!("Uploading {}", target.local.display()));
        mirror(target, &ignores)?;
        return create(target, TWO_WAY, &ignores);
    };
    if session.mode == MIRROR {
        output::step(&format!(
            "Finishing the interrupted upload of {}",
            target.local.display()
        ));
        terminate(&target.id)?;
        mirror(target, &ignores)?;
        return create(target, TWO_WAY, &ignores);
    }
    if session.paused {
        run(&["sync", "resume", &target.id])?;
    }
    if session.mode == LOCAL_WINS {
        flush(&target.id)?;
        terminate(&target.id)?;
        return create(target, TWO_WAY, &ignores);
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

pub fn sync_git(target: &SyncTarget) -> Result<()> {
    let ignores: Vec<String> = GIT_IGNORES
        .iter()
        .map(|pattern| pattern.to_string())
        .collect();
    if let Some(session) = session(&target.id)? {
        if session.ignore.paths == ignores {
            if session.paused {
                run(&["sync", "resume", &target.id])?;
            }
            return Ok(());
        }
        terminate(&target.id)?;
    }
    output::step(&format!(
        "Uploading the git history of {} (read-only for the agent)",
        target.local.display()
    ));
    mirror_progress(target, &ignores)
}

pub fn keep_local(target: &SyncTarget) -> Result<()> {
    ensure!(
        session(&target.id)?.is_some(),
        "{} is not syncing, run `sai chat` first",
        target.local.display()
    );
    resync(target, &ignores(target)?)
}

pub fn drop_if_present(id: &str) -> Result<()> {
    if session(id)?.is_some() {
        terminate(id)?;
    }
    Ok(())
}

pub fn finish(id: &str) -> Result<()> {
    let Some(session) = session(id)? else {
        return Ok(());
    };
    if session.paused {
        run(&["sync", "resume", id])?;
    }
    flush(id)?;
    terminate(id)
}

pub fn flush(id: &str) -> Result<()> {
    run(&["sync", "flush", id])
        .map(|_| ())
        .context("failed to finish the current sync, check the connection and try again")
}

/// Finishes what the agent deleted: mutagen stops at a deleted folder whose local copy
/// still holds what it does not sync. When that is only build output (`__pycache__` of
/// tests the agent removed, say), it goes too; files of the user stay, and the sync
/// issue tells about them
pub fn finish_deletions(target: &SyncTarget) -> Result<()> {
    let Some(session) = session(&target.id)? else {
        return Ok(());
    };
    for root in session.deleted_by_agent() {
        if remove_leftovers(&target.local, root) {
            output::info(&format!(
                "The agent deleted {root}, its build leftovers in {} went with it",
                target.local.display()
            ));
        }
    }
    Ok(())
}

/// Removes a deleted folder when nothing but build output is left in it
fn remove_leftovers(local: &Path, root: &str) -> bool {
    let path = local.join(root);
    if root.is_empty()
        || root
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path.is_dir()
        || !ignores::only_build_output(&path)
    {
        return false;
    }
    match fs::remove_dir_all(&path) {
        Ok(()) => true,
        Err(error) => {
            logs::line(&format!("cannot remove {}: {error}", path.display()));
            false
        }
    }
}

pub fn report_issues(id: &str) -> Result<()> {
    if let Some(session) = session(id)? {
        for issue in session.issues() {
            output::warning(&issue);
        }
    }
    Ok(())
}

pub fn pause_all() -> Result<()> {
    if !install::is_installed() {
        return Ok(());
    }
    let names: Vec<String> = sessions()?
        .into_iter()
        .filter(|session| session.name.starts_with(SAI_PREFIX) && !session.paused)
        .map(|session| session.name)
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    let mut flush = command()?
        .args(["sync", "flush"])
        .args(&names)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    while flush.try_wait()?.is_none() {
        if started.elapsed() > FLUSH_BEFORE_PAUSE {
            flush.kill().ok();
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let mut args = vec!["sync".to_owned(), "pause".to_owned()];
    args.extend(names);
    run(&args).map(|_| ())
}

pub fn resume_all() -> Result<()> {
    if !install::is_installed() {
        return Ok(());
    }
    let names: Vec<String> = sessions()?
        .into_iter()
        .filter(|session| session.name.starts_with(SAI_PREFIX) && session.paused)
        .map(|session| session.name)
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    let mut args = vec!["sync".to_owned(), "resume".to_owned()];
    args.extend(names);
    run(&args).map(|_| ())
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
    mirror_progress(target, ignores)?;
    terminate(&target.id)
}

fn mirror_progress(target: &SyncTarget, ignores: &[String]) -> Result<()> {
    output::info(&format!(
        "  about {} to upload, on a slow connection this takes a while",
        crate::project::size::human(crate::project::size::of(&target.local, &target.skip))
    ));
    create(target, MIRROR, ignores)?;
    wait_with_progress(&target.id)
}

fn wait_with_progress(id: &str) -> Result<()> {
    let mut child = command()?
        .args(["sync", "flush", id])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run mutagen")?;
    let started = Instant::now();
    let mut shown = String::new();
    let mut shown_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr).ok();
            }
            if !status.success() {
                logs::line(&format!("mutagen flush {id} failed: {}", stderr.trim()));
            }
            ensure!(
                status.success(),
                "the upload did not finish: {}",
                stderr.trim()
            );
            return Ok(());
        }
        thread::sleep(PROGRESS_INTERVAL);
        let Some(progress) = session(id)?.map(|session| session.progress()) else {
            continue;
        };
        if progress != shown || shown_at.elapsed() >= REPEAT_AFTER {
            output::info(&format!("  {progress} ({}s)", started.elapsed().as_secs()));
            shown = progress;
            shown_at = Instant::now();
        }
    }
}

fn command() -> Result<Command> {
    let mut command = Command::new(install::binary()?);
    command.env("MUTAGEN_DATA_DIRECTORY", install::data_directory()?);
    if let Some(dir) = ssh::windows_openssh() {
        command.env("MUTAGEN_SSH_PATH", dir);
    }
    Ok(command)
}

fn create(target: &SyncTarget, mode: &str, ignores: &[String]) -> Result<()> {
    let mut args = vec![
        "sync".to_owned(),
        "create".to_owned(),
        format!("--name={}", target.id),
        format!("--sync-mode={mode}"),
        "--ignore-vcs".to_owned(),
        "--symlink-mode=portable".to_owned(),
    ];
    args.extend(ignores.iter().map(|pattern| format!("--ignore={pattern}")));
    let local = target.local.display().to_string();
    let vps = format!("{HOST_ALIAS}:{}", target.remote);
    if mode == TWO_WAY {
        args.extend([vps, local]);
    } else {
        args.extend([local, vps]);
    }
    run(&args).map(|_| ())
}

fn ignores(target: &SyncTarget) -> Result<Vec<String>> {
    Ok(ignores::BUILD
        .iter()
        .map(|pattern| pattern.to_string())
        .chain(ignores::custom(&target.local)?)
        .chain(target.skip.iter().map(|path| ignores::anchored(path)))
        .chain(secrets::PATTERNS.iter().map(|pattern| pattern.to_string()))
        .collect())
}

fn run<S: AsRef<OsStr>>(args: &[S]) -> Result<String> {
    let output = command()?
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
    ensure!(
        output.status.success(),
        "{}",
        output::printable(stderr.trim())
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::remove_leftovers;
    use crate::testing::TempDir;

    #[test]
    fn removes_only_build_leftovers_of_deleted_folders() {
        let local = TempDir::new("leftovers");
        for path in [
            "tests/functional/__pycache__/a.pyc",
            "tests/unit/__pycache__/b.pyc",
            "tests/unit/test_mine.py",
            "keep/__pycache__/c.pyc",
        ] {
            let path = local.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
        assert!(remove_leftovers(&local.path, "tests/functional"));
        assert!(!local.join("tests/functional").exists());
        assert!(
            !remove_leftovers(&local.path, "tests/unit"),
            "a file of the user"
        );
        assert!(local.join("tests/unit/test_mine.py").exists());
        for bad in ["", "..", "tests/../keep", "/keep", "missing"] {
            assert!(!remove_leftovers(&local.path, bad), "{bad}");
        }
        assert!(local.join("keep/__pycache__/c.pyc").exists());
    }
}
