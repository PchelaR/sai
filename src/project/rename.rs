use anyhow::{Result, ensure};

use super::{Registry, validate_name};
use crate::{attach, mutagen, output, remote};

const RENAME: &str = include_str!("scripts/rename.sh");

pub fn run(old: &str, new: &str) -> Result<()> {
    validate_name(new)?;
    let mut registry = Registry::open()?;
    let project = registry.get(old)?.clone();
    ensure!(registry.find(new).is_none(), "project {new} already exists");
    remote::run(RENAME, &["check", old, new])?;
    for target in project.targets() {
        if let Some(session) = mutagen::session(&target.id)? {
            if !session.paused {
                mutagen::flush(&target.id)?;
            }
            mutagen::terminate(&target.id)?;
        }
    }
    remote::run(RENAME, &["move", old, new])?;
    registry.rename(old, new)?;
    registry.save()?;
    attach::ensure(&mut registry, new, None)?;
    output::info(&format!(
        "Renamed {old} to {new}. Talk to the agent with: sai chat {new}"
    ));
    Ok(())
}
