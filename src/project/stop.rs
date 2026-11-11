use anyhow::Result;

use crate::{mutagen, output, remote};

const STOP: &str = include_str!("scripts/stop.sh");

pub fn run(name: &str) -> Result<()> {
    super::validate_name(name)?;
    let id = super::id(name);
    remote::run(STOP, &[&id])?;
    if mutagen::session(&id)?.is_some_and(|session| !session.paused) {
        mutagen::pause(&id)?;
    }
    output::info(&format!("Stopped {name}. Resume with: sai chat <dir>"));
    Ok(())
}
