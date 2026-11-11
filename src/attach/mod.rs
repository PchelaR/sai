use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use crate::agent::Agent;
use crate::config::{self, Config};
use crate::project::{self, Folder, Project, Registry};
use crate::vps::SETUP_MARKER;
use crate::{fingerprint, image, mutagen, output, remote, secrets, snapshot, ssh, tools};

const PREPARE_PROJECT: &str = include_str!("scripts/prepare_project.sh");
const REMOVE_SECRETS: &str = include_str!("scripts/remove_secrets.sh");
const ENSURE_IMAGE: &str = concat!(
    include_str!("../image/build.sh"),
    include_str!("scripts/ensure_image.sh")
);
const ENSURE_CONTAINER: &str = include_str!("scripts/ensure_container.sh");
const FREE_SPACE: &str = include_str!("scripts/free_space.sh");
const RESERVE: u64 = 2 * 1024 * 1024 * 1024;

pub struct Attached {
    pub project: Project,
    pub agent: Agent,
}

pub fn run(
    name: &str,
    dir_path: &str,
    agent: Option<Agent>,
    alias: Option<&str>,
    approve_large: &dyn Fn(&Path, u64) -> Result<bool>,
) -> Result<()> {
    let mut registry = Registry::open()?;
    let path = project::local_path(dir_path)?;
    let folder = match alias {
        Some(alias) => alias.to_owned(),
        None => project::folder_name(&path)?,
    };
    match registry.find(name) {
        None => registry.create(name, agent.unwrap_or_default())?,
        Some(existing) => {
            if let (Some(requested), Some(bound)) = (agent, existing.agent)
                && requested != bound
            {
                bail!("{name} is bound to {bound}, switch it with `sai change {requested} {name}`");
            }
        }
    }
    let already = registry
        .get(name)?
        .folders
        .iter()
        .find(|existing| project::same_path(&existing.path, &path))
        .map(|existing| existing.name.clone());
    let folder = match already {
        Some(existing) => existing,
        None => {
            if !has_room(&path, approve_large)? {
                output::info("Nothing was attached");
                return Ok(());
            }
            registry.add_folder(
                name,
                Folder {
                    name: folder.clone(),
                    path: path.clone(),
                },
            )?;
            registry.save()?;
            folder
        }
    };
    let attached = ensure(&mut registry, name, agent)?;
    output::info(&format!(
        "{} is attached to {name} as /workspace/{folder}, the agent is {}. Talk to it with: sai chat {name}",
        path.display(),
        attached.agent
    ));
    Ok(())
}

pub fn current_folder(registry: &mut Registry, agent: Option<Agent>) -> Result<String> {
    if let Ok(project) = registry.resolve(None) {
        return Ok(project.name.clone());
    }
    let path = project::current_dir()?;
    let home = etcetera::home_dir().ok();
    ensure!(
        path.parent().is_some() && home.is_none_or(|home| !project::same_path(&home, &path)),
        "run sai inside a project folder or name the project, see `sai list`"
    );
    let name = project::folder_name(&path)?;
    ensure!(
        registry.find(&name).is_none(),
        "{} is not attached yet and a project named {name} already exists. Add the folder to it with `sai attach {name} .` or create another project with `sai attach <new-name> .`",
        path.display()
    );
    registry.create(&name, agent.unwrap_or_default())?;
    registry.add_folder(
        &name,
        Folder {
            name: name.clone(),
            path: path.clone(),
        },
    )?;
    registry.save()?;
    output::info(&format!(
        "Created project {name} with {}. Add more folders with: sai attach {name} <folder>",
        path.display()
    ));
    Ok(name)
}

pub fn ensure(registry: &mut Registry, name: &str, requested: Option<Agent>) -> Result<Attached> {
    ssh::install_host_alias(&Config::load()?)?;
    mutagen::ensure_installed()?;
    let id = project::id(name);
    let remote_path = project::remote_path(name);

    let label = remote::output(PREPARE_PROJECT, &[SETUP_MARKER, &id, &remote_path])?;
    let project = registry.get(name)?.clone();
    let agent = choose_agent(
        name,
        requested,
        project.agent.or(Agent::parse(label.trim())),
    )?;
    if project.agent != Some(agent) {
        registry.set_agent(name, agent)?;
        registry.save()?;
    }

    tools::install(&config::instructions()?)?;
    for target in project.targets() {
        mutagen::sync(&target)?;
    }
    let mut secret_args = vec![remote_path.as_str()];
    secret_args.extend(secrets::names());
    remote::run(REMOVE_SECRETS, &secret_args)?;
    snapshot::take(name)?;

    let image = image::agent(agent);
    remote::run(
        ENSURE_IMAGE,
        &[
            &image::base(),
            &image,
            &image::encoded_agent(agent),
            agent.name(),
        ],
    )?;

    let mounts = tools::read_only_mounts(agent);
    let spec = fingerprint::of(&format!("{ENSURE_CONTAINER}{}", mounts.join("\n")));
    let mut container_args = vec![
        id.as_str(),
        remote_path.as_str(),
        name,
        image.as_str(),
        spec.as_str(),
        agent.name(),
    ];
    container_args.extend(mounts.iter().map(String::as_str));
    remote::run(ENSURE_CONTAINER, &container_args)?;

    for target in project.targets() {
        mutagen::report_issues(&target.id)?;
    }
    let project = registry.get(name)?.clone();
    Ok(Attached { project, agent })
}

pub fn ask_large(path: &Path, size: u64) -> Result<bool> {
    Ok(dialoguer::Confirm::new()
        .with_prompt(large_question(path, size))
        .default(false)
        .interact()?)
}

pub fn large_question(path: &Path, size: u64) -> String {
    format!(
        "{} has {} of files to upload to the VPS. Attach it anyway?",
        path.display(),
        project::size::human(size)
    )
}

fn has_room(path: &Path, approve_large: &dyn Fn(&Path, u64) -> Result<bool>) -> Result<bool> {
    output::step(&format!("Checking the size of {}", path.display()));
    let size = project::size::of(path);
    if project::size::is_large(size) && !approve_large(path, size)? {
        return Ok(false);
    }
    ssh::install_host_alias(&Config::load()?)?;
    let free: u64 = remote::output(FREE_SPACE, &[])?
        .trim()
        .parse()
        .context("could not read the free space on the VPS")?;
    let needed = size * 2 + RESERVE;
    ensure!(
        free >= needed,
        "{} needs about {} on the VPS (the files, their first snapshot and a reserve), only {} is free. Free space with `sai rm <project> --purge` or use a bigger VPS",
        path.display(),
        project::size::human(needed),
        project::size::human(free)
    );
    Ok(true)
}

fn choose_agent(name: &str, requested: Option<Agent>, bound: Option<Agent>) -> Result<Agent> {
    match (requested, bound) {
        (Some(requested), Some(bound)) if requested != bound => {
            bail!("{name} is bound to {bound}, switch it with `sai change {requested} {name}`")
        }
        (requested, bound) => Ok(requested.or(bound).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::choose_agent;
    use crate::agent::Agent;

    #[test]
    fn uses_the_default_for_new_projects() {
        assert_eq!(choose_agent("app", None, None).unwrap(), Agent::Claude);
        assert_eq!(
            choose_agent("app", Some(Agent::Codex), None).unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn keeps_the_bound_agent() {
        assert_eq!(
            choose_agent("app", None, Some(Agent::Codex)).unwrap(),
            Agent::Codex
        );
        assert_eq!(
            choose_agent("app", Some(Agent::Codex), Some(Agent::Codex)).unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn refuses_a_second_agent() {
        let error = choose_agent("app", Some(Agent::Codex), Some(Agent::Claude)).unwrap_err();
        assert!(error.to_string().contains("sai change codex app"));
    }
}
