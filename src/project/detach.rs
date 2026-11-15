use std::path::PathBuf;

use anyhow::{Context, Result};
use dialoguer::Confirm;

use super::{Folder, Project, Registry, local_path, same_path};
use crate::{mutagen, output, remote, snapshot};

const DETACH: &str = include_str!("scripts/detach.sh");

pub fn run_with_confirmation(name: &str, dir_path: &str) -> Result<()> {
    let mut registry = Registry::open()?;
    let project = registry.get(name)?.clone();
    let folder = find(&project, dir_path)?;
    let confirmed = Confirm::new()
        .with_prompt(format!(
            "Detach {} from {name}? Local files stay, the copy on the VPS is removed (it stays in snapshots)",
            folder.path.display()
        ))
        .default(false)
        .interact()?;
    if !confirmed {
        output::info("Nothing was detached");
        return Ok(());
    }
    detach(&mut registry, &project, &folder)
}

pub fn run(name: &str, dir_path: &str) -> Result<()> {
    let mut registry = Registry::open()?;
    let project = registry.get(name)?.clone();
    let folder = find(&project, dir_path)?;
    detach(&mut registry, &project, &folder)
}

pub fn find(project: &Project, dir_path: &str) -> Result<Folder> {
    let path = local_path(dir_path).unwrap_or_else(|_| PathBuf::from(dir_path));
    project
        .folders
        .iter()
        .find(|folder| same_path(&folder.path, &path))
        .or_else(|| {
            project
                .folders
                .iter()
                .find(|folder| folder.name == dir_path)
        })
        .cloned()
        .with_context(|| {
            format!(
                "{dir_path} is not attached to {}, its folders are: {}",
                project.name,
                project.folder_names().join(", ")
            )
        })
}

fn detach(registry: &mut Registry, project: &Project, folder: &Folder) -> Result<()> {
    let name = project.name.as_str();
    let target = project.target(folder);
    mutagen::finish(&target.id)?;
    for id in project.git_session_ids(folder)? {
        mutagen::drop_if_present(&id)?;
    }
    snapshot::take(name)?;
    remote::run(DETACH, &[&target.remote])?;
    registry.remove_folder(name, &folder.path)?;
    registry.save()?;
    output::info(&format!(
        "Detached {} from {name}. Local files are untouched, the copy on the VPS was removed and stays in snapshots",
        folder.path.display()
    ));
    if project.folders.len() == 1 {
        output::info(&format!(
            "{name} has no folders now. Add one with `sai attach {name} <folder>` or remove the project with `sai rm {name}`"
        ));
    }
    Ok(())
}
