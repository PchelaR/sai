use anyhow::Result;

use super::Project;
use crate::agent::Agent;
use crate::{attach, output, remote};

const RELEASE_AGENT: &str = include_str!("scripts/release_agent.sh");

pub fn run(dir_path: &str, agent: Agent) -> Result<()> {
    let project = Project::open(dir_path)?;
    remote::run(RELEASE_AGENT, &[&project.id(), agent.name()])?;
    let attached = attach::ensure(dir_path, Some(agent))?;
    output::info(&format!(
        "{} is now bound to {}. Talk to it with: sai chat {dir_path}",
        attached.project.name, attached.agent
    ));
    Ok(())
}
