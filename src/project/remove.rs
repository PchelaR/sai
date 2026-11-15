use anyhow::{Result, ensure};
use dialoguer::{Confirm, Input};

use super::{Registry, id, remote_path, validate_name};
use crate::{mutagen, output, remote, snapshot};

const REMOVE: &str = concat!(
    include_str!("scripts/remove.sh"),
    include_str!("scripts/remove_unused_images.sh")
);

pub fn run_with_confirmation(name: &str, purge: bool) -> Result<()> {
    let confirmed = if purge {
        let typed: String = Input::new()
            .with_prompt(format!(
                "This deletes {name} with its files and snapshots on the VPS forever, local folders stay. Type {name} to confirm"
            ))
            .allow_empty(true)
            .interact_text()?;
        typed.trim() == name
    } else {
        Confirm::new()
            .with_prompt(format!(
                "Remove {name}? Syncing stops, files and snapshots stay on the VPS, local folders stay"
            ))
            .default(false)
            .interact()?
    };
    if !confirmed {
        output::info("Nothing was removed");
        return Ok(());
    }
    run(name, purge)
}

pub fn run(name: &str, purge: bool) -> Result<()> {
    validate_name(name)?;
    let mut registry = Registry::open()?;
    let project = registry.find(name).cloned();
    if let Some(project) = &project {
        for id in project.session_ids()? {
            mutagen::drop_if_present(&id)?;
        }
    }
    let mode = if purge { "purge" } else { "keep" };
    let found = remote::output(REMOVE, &[&id(name), &remote_path(name), mode])?;
    ensure!(
        project.is_some() || found.trim() == "found",
        "no project named {name}"
    );
    registry.remove(name);
    registry.save()?;
    if purge {
        snapshot::purge(name)?;
        output::info(&format!(
            "Removed {name}, its files and snapshots on the VPS. Local folders are untouched"
        ));
    } else {
        output::info(&format!(
            "Removed {name}. Its files and snapshots stay on the VPS, use --purge to delete them. Local folders are untouched"
        ));
    }
    Ok(())
}
