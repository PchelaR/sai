use anyhow::Result;

use crate::{mutagen, remote};

const STOP: &str = include_str!("scripts/stop.sh");

pub fn run(name: &str) -> Result<()> {
    super::validate_name(name)?;
    let id = super::id(name);
    remote::run(STOP, &[&id])?;
    if mutagen::session(&id)?.is_some_and(|session| !session.paused) {
        mutagen::pause(&id)?;
    }
    println!("Stopped {name}. Resume with: sai chat <dir>");
    Ok(())
}
