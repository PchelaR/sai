use anyhow::Result;

use crate::agent::Agent;
use crate::{attach, quote, remote};

pub fn run(dir_path: &str, agent: Option<Agent>) -> Result<()> {
    let attached = attach::ensure(dir_path, agent)?;
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
