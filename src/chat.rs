use anyhow::Result;

use crate::agent::Agent;
use crate::project::Registry;
use crate::snapshot::review;
use crate::{attach, console, output, quote, remote};

pub fn run(name: Option<&str>, agent: Option<Agent>) -> Result<()> {
    let mut registry = Registry::open()?;
    let name = match name {
        Some(name) => registry.get(name)?.name.clone(),
        None => attach::current_folder(
            &mut registry,
            agent,
            &attach::ask_new_items,
            &attach::ask_git,
        )?,
    };
    let attached = attach::ensure(&mut registry, &name, agent)?;
    if console::is_msys() {
        output::warning(console::MSYS_ADVICE);
    }
    let command = format!(
        "{} || {{ status=$?; echo; echo \"The agent exited with status $status, press Enter to close\"; read line; }}",
        attached.agent.command()
    );
    let result = remote::interactive(&format!(
        "exec docker exec -it {} tmux new -A -s {} {}",
        quote::shell(&attached.project.id()),
        attached.agent.name(),
        quote::shell(&command)
    ));
    review::after_chat(&name);
    result
}
