use anyhow::Result;

use crate::agent::Agent;
use crate::{output, remote};

const UPDATE_IMAGE: &str = include_str!("scripts/update_image.sh");

pub fn run(agent: Agent) -> Result<()> {
    let image = agent.image()?;
    output::step("Building agent image with the latest versions (may take up to an hour)");
    remote::run(
        UPDATE_IMAGE,
        &[&image.reference(), &image.encoded_dockerfile()],
    )?;
    println!(
        "Agent image updated. Projects switch to it on the next `sai chat`, running agent sessions keep the old one until they end"
    );
    Ok(())
}
