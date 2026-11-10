use anyhow::{Result, ensure};

use crate::{mutagen, remote};

const REMOVE: &str = include_str!("scripts/remove.sh");

pub fn run(name: &str, purge: bool) -> Result<()> {
    super::validate_name(name)?;
    let id = super::id(name);
    let synced = mutagen::session(&id)?.is_some();
    if synced {
        mutagen::terminate(&id)?;
    }
    let mode = if purge { "purge" } else { "keep" };
    let found = remote::output(REMOVE, &[&id, &super::remote_path(name), mode])?;
    ensure!(synced || found.trim() == "found", "no project named {name}");
    if purge {
        println!("Removed {name} and its files on the VPS");
    } else {
        println!("Removed {name}. Its files stay on the VPS, use --purge to delete them");
    }
    Ok(())
}
