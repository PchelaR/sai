pub mod chats;
mod login;

use std::io::IsTerminal;

use anyhow::{Result, ensure};
use dialoguer::{Confirm, Select};

use crate::agent::{Agent, Start};
use crate::project::{self, Registry};
use crate::snapshot::review;
use crate::{attach, console, output, quote, remote, tools};

pub fn run(name: Option<&str>, agent: Option<Agent>, start: Start) -> Result<()> {
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
    let lock = project::lock::hold(&name)?;
    let attached = attach::ensure(&mut registry, &name, agent)?;
    let agent = attached.agent;
    attach::old_chats(&name, agent)?;
    let start = match start {
        Start::Ask => ask(&name, agent)?,
        other => other,
    };
    let switching = match &start {
        Start::New => true,
        Start::Open(chat) => {
            ensure!(chats::valid_id(chat), "invalid chat id {chat:?}");
            chats::load(&name, agent)?
                .running()
                .is_none_or(|running| &running.id != chat)
        }
        _ => false,
    };
    if switching {
        let current = chats::state(&name, agent)?;
        if current.clients > 0 && !confirm_switch(&name)? {
            output::info("The current chat stays open");
            return Ok(());
        }
        chats::stop_if_idle(&name, agent)?;
    }
    drop(lock);
    if console::is_msys() {
        output::warning(console::MSYS_ADVICE);
    }
    let command = format!(
        "{} || {{ status=$?; echo; echo \"The agent exited with status $status, press Enter to close\"; read line; }}",
        agent.command(&start)
    );
    let watcher = login::watch(&attached.project.id(), agent);
    let container = quote::shell(&attached.project.id());
    // An agent started by an older sai keeps its tmux settings, reload them on every attach
    let result = remote::interactive(&format!(
        "docker exec {container} tmux source-file {} >/dev/null 2>&1; exec docker exec -it {container} tmux new -A -s {} {}",
        tools::TMUX_TARGET,
        agent.name(),
        quote::shell(&command)
    ));
    drop(watcher);
    review::after_chat(&name);
    // After the chat: before it, tmux would cover the tips at once
    crate::windows_terminal::offer_once();
    result
}

pub fn list(name: Option<&str>, remove: Option<usize>, show: Option<usize>) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?;
    let agent = project.agent.unwrap_or_default();
    let found = chats::load(&project.name, agent)?;
    if let Some(number) = show {
        let chat = chats::pick(&found, number)?;
        output::info(&chats::show(&project.name, agent, &chat.id)?);
        return Ok(());
    }
    if let Some(number) = remove {
        let chat = chats::pick(&found, number)?;
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Delete the chat \"{}\"? This cannot be undone",
                chat.title
            ))
            .default(false)
            .interact()?;
        if confirmed {
            chats::remove(&project.name, agent, &chat.id)?;
            output::info("Deleted");
        } else {
            output::info("Nothing was deleted");
        }
        return Ok(());
    }
    if found.list.is_empty() {
        output::info(&format!(
            "No chats in {} yet, start one with: sai chat {}",
            project.name, project.name
        ));
        return Ok(());
    }
    for (index, chat) in found.list.iter().enumerate() {
        let marker = if found.running().is_some_and(|running| running.id == chat.id) {
            format!("  [{}]", found.state)
        } else {
            String::new()
        };
        output::info(&format!("{:>3}  {}{marker}", index + 1, chat.label()));
    }
    output::info(&format!(
        "Open one with: sai chat {name} --open <number>, read it with: sai chats {name} --show <number>, delete with: sai chats {name} --rm <number>",
        name = project.name
    ));
    Ok(())
}

fn confirm_switch(name: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    Ok(Confirm::new()
        .with_prompt(format!(
            "The current chat of {name} is open in another terminal, switch anyway? That terminal is disconnected from it"
        ))
        .default(false)
        .interact()?)
}

fn ask(name: &str, agent: Agent) -> Result<Start> {
    let found = chats::load(name, agent)?;
    if found.list.len() < 2 || !std::io::stdin().is_terminal() {
        return Ok(Start::Last);
    }
    let labels: Vec<String> = found.list.iter().map(chats::Chat::label).collect();
    let index = Select::new()
        .with_prompt(format!("Chats in {name}"))
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(Start::Open(found.list[index].id.clone()))
}

pub fn open_number(name: Option<&str>, agent: Option<Agent>, number: usize) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?;
    let found = chats::load(&project.name, project.agent.unwrap_or_default())?;
    let chat = chats::pick(&found, number)?.id.clone();
    let name = project.name.clone();
    drop(registry);
    run(Some(&name), agent, Start::Open(chat))
}
