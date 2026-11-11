use anyhow::Result;

use crate::agent::Agent;
use crate::project::Registry;
use crate::{attach, quote, remote};

pub fn run(name: Option<&str>, agent: Option<Agent>) -> Result<()> {
    let mut registry = Registry::open()?;
    let name = match name {
        Some(name) => registry.get(name)?.name.clone(),
        None => attach::current_folder(&mut registry, agent)?,
    };
    let attached = attach::ensure(&mut registry, &name, agent)?;
    let command = format!(
        "{} || {{ status=$?; echo; echo \"The agent exited with status $status, press Enter to close\"; read line; }}",
        attached.agent.command()
    );
    remote::interactive(&format!(
        "exec docker exec -it {} tmux new -A -s {} {}",
        quote::shell(&attached.project.id()),
        attached.agent.name(),
        quote::shell(&command)
    ))
}
