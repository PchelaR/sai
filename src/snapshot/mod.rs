use std::path::{Component, Path};

use anyhow::{Result, ensure};
use dialoguer::Confirm;

use crate::project::Project;
use crate::{mutagen, remote, tools};

const RUN: &str = include_str!("run.sh");

pub fn take(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "take", name])
}

pub fn purge(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "purge", name])
}

pub fn list(dir_path: &str) -> Result<()> {
    let project = Project::open(dir_path)?;
    remote::run(RUN, &[&tools::snapshots_path(), "list", &project.name])
}

pub fn restore(dir_path: &str, snapshot: &str, path: Option<&str>) -> Result<()> {
    let project = Project::open(dir_path)?;
    ensure!(
        mutagen::session(&project.id())?.is_some(),
        "{} is not attached, run `sai chat {dir_path}` first",
        project.name
    );
    if let Some(path) = path {
        validate_path(path)?;
    }

    let target = path.unwrap_or("the whole project");
    let confirmed = Confirm::new()
        .with_prompt(format!(
            "Restore {target} of {} from {snapshot}? Current files are snapshotted first",
            project.name
        ))
        .default(false)
        .interact()?;
    if !confirmed {
        println!("Cancelled");
        return Ok(());
    }

    remote::run(
        RUN,
        &[
            &tools::snapshots_path(),
            "restore",
            &project.name,
            snapshot,
            path.unwrap_or_default(),
        ],
    )?;
    println!("Restored {target} from {snapshot}. Changes are syncing to your local folder");
    mutagen::report_issues(&project.id())
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
    use super::validate_path;

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
