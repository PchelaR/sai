use anyhow::Result;

use super::Registry;
use crate::agent::Agent;
use crate::{attach, output, remote};

const RELEASE_AGENT: &str = concat!(
    include_str!("scripts/release_agent.sh"),
    include_str!("scripts/remove_unused_images.sh")
);

pub fn run(name: Option<&str>, agent: Agent) -> Result<()> {
    let mut registry = Registry::open()?;
    let project = registry.resolve(name)?.clone();
    remote::run(RELEASE_AGENT, &[&project.id(), agent.name()])?;
    registry.set_agent(&project.name, agent)?;
    registry.save()?;
    let attached = attach::ensure(&mut registry, &project.name, Some(agent))?;
    output::info(&format!(
        "{} is now bound to {}. Talk to it with: sai chat {}",
        project.name, attached.agent, project.name
    ));
    Ok(())
}
