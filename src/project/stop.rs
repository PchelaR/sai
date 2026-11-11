use anyhow::Result;

use super::{Registry, id};
use crate::{mutagen, output, remote};

const STOP: &str = include_str!("scripts/stop.sh");

pub fn run(name: Option<&str>) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?;
    let name = project.name.as_str();
    remote::run(STOP, &[&id(name)])?;
    for target in project.targets() {
        if mutagen::session(&target.id)?.is_some_and(|session| !session.paused) {
            mutagen::pause(&target.id)?;
        }
    }
    output::info(&format!("Stopped {name}. Resume with: sai chat {name}"));
    Ok(())
}
