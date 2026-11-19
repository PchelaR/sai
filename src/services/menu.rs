use std::io::IsTerminal;

use anyhow::{Result, ensure};
use dialoguer::{Confirm, Input, Select};

use super::{Kind, Service, split_names};
use crate::output;
use crate::project::{Registry, lock};

enum Choice {
    Service(usize),
    Add(Kind),
    Done,
}

/// The services of a project: their state, adding one from a template, start, stop,
/// restart, change and remove
pub fn run(name: Option<&str>) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal(),
        "managing services needs a terminal, run `sai services` in one"
    );
    let project = Registry::open()?.resolve(name)?.name.clone();
    output::info(&format!(
        "Services of {project} run next to its agent, it reaches them on localhost. Their data lives on the VPS and is not part of the snapshots"
    ));
    loop {
        let services = Registry::open()?.get(&project)?.services.clone();
        let states = super::status(&project).unwrap_or_default();
        let mut labels = Vec::new();
        let mut choices = Vec::new();
        for (index, service) in services.iter().enumerate() {
            let container = service.container(&project);
            let state = states
                .iter()
                .find(|(name, _)| *name == container)
                .map(|(_, state)| state.as_str());
            labels.push(format!(
                "{}  ({})",
                service.label(),
                describe(service, state)
            ));
            choices.push(Choice::Service(index));
        }
        for kind in Kind::ALL {
            if !services.iter().any(|service| service.kind == kind) {
                labels.push(format!("+ add {}", kind.title()));
                choices.push(Choice::Add(kind));
            }
        }
        labels.push("done".to_owned());
        choices.push(Choice::Done);
        // Esc or q leaves the menu, like "done"
        let Some(picked) = Select::new()
            .with_prompt(format!("Services of {project} (Esc to leave)"))
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        match choices[picked] {
            Choice::Service(index) => manage(&project, index)?,
            Choice::Add(kind) => add(&project, kind)?,
            Choice::Done => return Ok(()),
        }
    }
}

fn describe(service: &Service, state: Option<&str>) -> &'static str {
    match (service.stopped, state) {
        (true, _) => "stopped",
        (false, Some("running")) => "running",
        (false, Some("restarting")) => "restarting, check its settings",
        (false, Some(_)) => "not running",
        (false, None) => "starts with the next sai chat",
    }
}

fn add(project: &str, kind: Kind) -> Result<()> {
    let Some(service) = edit(Service::new(kind))? else {
        return Ok(());
    };
    save(project, |services| services.push(service.clone()))?;
    output::info(&format!("Added. {}", service.connection()));
    output::info(
        "The agent finds this in its instructions in new chats, tell a running chat yourself",
    );
    Ok(())
}

fn manage(project: &str, index: usize) -> Result<()> {
    let services = Registry::open()?.get(project)?.services.clone();
    let Some(service) = services.get(index).cloned() else {
        return Ok(());
    };
    let toggle = if service.stopped { "start" } else { "stop" };
    let actions = [
        "show how to connect",
        toggle,
        "restart",
        "change settings",
        "remove with its data",
        "back",
    ];
    let Some(picked) = Select::new()
        .with_prompt(service.label())
        .items(actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    match actions[picked] {
        "show how to connect" => output::info(&service.connection()),
        "start" | "stop" => {
            let stopped = !service.stopped;
            save(project, |services| services[index].stopped = stopped)?;
        }
        "restart" => {
            ensure!(!service.stopped, "{} is stopped, start it", service.label());
            let _lock = lock::hold(project)?;
            super::restart(project, &service)?;
        }
        "change settings" => {
            let old_volume = service.volume();
            let Some(changed) = edit(service.clone())? else {
                return Ok(());
            };
            if changed.volume() != old_volume {
                output::warning(&format!(
                    "{} starts with empty data. The data of {} stays on the VPS and comes back if you switch back",
                    changed.label(),
                    service.label()
                ));
            }
            save(project, |services| services[index] = changed.clone())?;
        }
        "remove with its data" => {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "Remove {} with all its data? This cannot be undone. To keep the data, stop it instead",
                    service.label()
                ))
                .default(false)
                .interact()?;
            if confirmed {
                let guard = lock::hold(project)?;
                super::remove(project, &service)?;
                drop(guard);
                save(project, |services| {
                    services.remove(index);
                })?;
                output::info(&format!("Removed {}", service.label()));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Asks for the settings of a service until they are valid. None: the user gave up
fn edit(mut service: Service) -> Result<Option<Service>> {
    let title = service.kind.title();
    loop {
        service.version = Input::new()
            .with_prompt(format!("{title} version (the tag of the Docker image)"))
            .default(service.version.clone())
            .interact_text()?;
        if service.kind.has_user() {
            service.user = Input::new()
                .with_prompt("User")
                .default(service.user.clone())
                .interact_text()?;
            service.password = Input::new()
                .with_prompt("Password")
                .default(service.password.clone())
                .interact_text()?;
            let names: String = Input::new()
                .with_prompt(format!(
                    "{}{}, separated by commas",
                    service.kind.names_label()[..1].to_uppercase(),
                    &service.kind.names_label()[1..]
                ))
                .default(service.names.join(", "))
                .interact_text()?;
            service.names = split_names(&names);
        } else {
            service.password = Input::new()
                .with_prompt("Password (empty for none)")
                .default(service.password.clone())
                .allow_empty(true)
                .interact_text()?;
        }
        match service.validate() {
            Ok(()) => return Ok(Some(service)),
            Err(error) => {
                output::warning(&format!("{error:#}"));
                let again = Confirm::new()
                    .with_prompt("Fix it?")
                    .default(true)
                    .interact()?;
                if !again {
                    return Ok(None);
                }
            }
        }
    }
}

/// Changes the list, saves it and brings the VPS in line
fn save(project: &str, change: impl FnOnce(&mut Vec<Service>)) -> Result<()> {
    let _lock = lock::hold(project)?;
    let mut registry = Registry::open()?;
    let mut services = registry.get(project)?.services.clone();
    change(&mut services);
    registry.set_services(project, services.clone())?;
    registry.save()?;
    super::apply(project, &services)
}
