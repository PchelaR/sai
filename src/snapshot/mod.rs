use std::path::{Component, Path};

use anyhow::{Result, ensure};
use dialoguer::Confirm;

use crate::project::Project;
use crate::{mutagen, remote, tools};

const RUN: &str = include_str!("run.sh");

pub fn take(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "take", name])
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
        ensure!(
            Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "path must be relative to the project folder: {path}"
        );
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
