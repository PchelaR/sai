use anyhow::Result;
use clap::ValueEnum;

use crate::agent::Agent;
use crate::{image, output, remote};

const UPDATE_IMAGES: &str = include_str!("scripts/update_images.sh");

pub fn run() -> Result<()> {
    output::step("Rebuilding images with the latest versions (may take up to an hour)");
    let mut args = vec![image::base(), image::encoded_base()];
    for agent in Agent::value_variants() {
        args.push(image::agent(*agent));
        args.push(image::encoded_agent(*agent));
    }
    remote::run(
        UPDATE_IMAGES,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    println!(
        "Images updated. Projects switch to them on the next `sai chat`, running agent sessions keep the old ones until they end"
    );
    Ok(())
}
