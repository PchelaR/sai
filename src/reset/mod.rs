use anyhow::{Result, ensure};
use dialoguer::Input;

use crate::config::Config;
use crate::{mutagen, output, remote};

const RESET: &str = include_str!("scripts/reset.sh");

pub fn run(all: bool) -> Result<()> {
    let config = Config::load()?;
    let images = if all {
        "all images, including the base one"
    } else {
        "agent images (the base image is kept)"
    };
    println!(
        "This deletes everything sai created on {}: agent containers, agent logins, project files, snapshots and {images}.",
        config.ip
    );
    let confirmation: String = Input::new()
        .with_prompt(format!("Type {} to confirm", config.ip))
        .interact_text()?;
    ensure!(
        confirmation.trim() == config.ip,
        "confirmation did not match, nothing was deleted"
    );

    output::step("Stopping sync");
    mutagen::terminate_all()?;
    output::step("Deleting everything on the VPS");
    remote::run(RESET, &[if all { "all" } else { "keep-base" }])?;

    println!(
        "{} is reset. Your local folders are untouched, `sai chat` uploads them again",
        config.ip
    );
    Ok(())
}
