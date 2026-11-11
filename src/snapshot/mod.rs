use std::path::{Component, Path};

use anyhow::{Result, ensure};
use dialoguer::Confirm;

use crate::project::{Project, Registry};
use crate::{mutagen, output, remote, tools};

const RUN: &str = include_str!("run.sh");
const STAMP_LENGTH: usize = 16;
const STAMP_DIGITS: usize = 14;

pub fn take(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "take", name])
}

pub fn purge(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "purge", name])
}

pub fn list(name: Option<&str>) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?;
    remote::run(RUN, &[&tools::snapshots_path(), "list", &project.name])
}

pub fn stamps(name: &str) -> Result<Vec<String>> {
    let output = remote::output(RUN, &[&tools::snapshots_path(), "list", name])?;
    Ok(output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|stamp| is_stamp(stamp))
        .map(str::to_owned)
        .collect())
}

pub fn restore_with_confirmation(
    name: Option<&str>,
    snapshot: &str,
    path: Option<&str>,
) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?.clone();
    let path = checked_path(&project, path)?;
    let path = path.as_deref();
    let target = path.unwrap_or("the whole project");
    let confirmed = Confirm::new()
        .with_prompt(format!(
            "Restore {target} of {} from {snapshot}? Current files are snapshotted first",
            project.name
        ))
        .default(false)
        .interact()?;
    if !confirmed {
        output::info("Cancelled");
        return Ok(());
    }
    restore(&project, snapshot, path)
}

pub fn restore(project: &Project, snapshot: &str, path: Option<&str>) -> Result<()> {
    ensure!(
        project
            .targets()
            .iter()
            .any(|target| mutagen::session(&target.id).is_ok_and(|session| session.is_some())),
        "{} is not syncing, run `sai chat {}` first",
        project.name,
        project.name
    );
    let path = checked_path(project, path)?;
    let path = path.as_deref();
    let mut args = vec![
        tools::snapshots_path(),
        "restore".to_owned(),
        project.name.clone(),
        snapshot.to_owned(),
        path.unwrap_or_default().to_owned(),
    ];
    args.extend(project.folder_names().iter().map(|name| (*name).to_owned()));
    remote::run(RUN, &args.iter().map(String::as_str).collect::<Vec<_>>())?;
    output::info(&format!(
        "Restored {} from {snapshot}. Changes are syncing to your local folders",
        path.unwrap_or("the whole project")
    ));
    for target in project.targets() {
        mutagen::report_issues(&target.id)?;
    }
    Ok(())
}

fn checked_path(project: &Project, path: Option<&str>) -> Result<Option<String>> {
    let Some(path) = path.map(|path| path.replace('\\', "/")) else {
        return Ok(None);
    };
    validate_path(&path)?;
    let folder = path.split('/').next().unwrap_or_default();
    ensure!(
        project.folder_names().contains(&folder),
        "{path} must start with one of the project folders: {}",
        project.folder_names().join(", ")
    );
    Ok(Some(path))
}

fn is_stamp(text: &str) -> bool {
    text.len() == STAMP_LENGTH
        && text.ends_with('Z')
        && text.chars().filter(char::is_ascii_digit).count() == STAMP_DIGITS
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "path must be relative to the project folder: {path}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_stamp, validate_path};

    #[test]
    fn recognizes_snapshot_stamps() {
        assert!(is_stamp("20261110T063000Z"));
        assert!(!is_stamp("No"));
        assert!(!is_stamp("2026-11-10"));
    }

    #[test]
    fn accepts_paths_inside_the_project() {
        assert!(validate_path("src/main.rs").is_ok());
        assert!(validate_path("README.md").is_ok());
    }

    #[test]
    fn rejects_paths_leaving_the_project() {
        for path in ["", "/etc/passwd", "../secret", "src/../../x", "./src"] {
            assert!(validate_path(path).is_err(), "{path} must be rejected");
        }
    }
}
