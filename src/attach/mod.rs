use anyhow::{Result, bail};

use crate::agent::Agent;
use crate::config::{self, Config};
use crate::project::Project;
use crate::vps::SETUP_MARKER;
use crate::{fingerprint, image, mutagen, output, remote, secrets, snapshot, ssh, tools};

const PREPARE_PROJECT: &str = include_str!("scripts/prepare_project.sh");
const REMOVE_SECRETS: &str = include_str!("scripts/remove_secrets.sh");
const ENSURE_IMAGE: &str = concat!(
    include_str!("../image/build.sh"),
    include_str!("scripts/ensure_image.sh")
);
const ENSURE_CONTAINER: &str = include_str!("scripts/ensure_container.sh");

pub struct Attached {
    pub project: Project,
    pub agent: Agent,
}

pub fn run(dir_path: &str, agent: Option<Agent>) -> Result<()> {
    let attached = ensure(dir_path, agent)?;
    output::info(&format!(
        "{} is attached to {}. Talk to it with: sai chat {dir_path}",
        attached.project.name, attached.agent
    ));
    Ok(())
}

pub fn ensure(dir_path: &str, requested: Option<Agent>) -> Result<Attached> {
    ssh::install_host_alias(&Config::load()?)?;
    mutagen::ensure_installed()?;
    let project = Project::open(dir_path)?;
    let id = project.id();
    let local_path = project.local_path.display().to_string();
    let remote_path = project.remote_path();

    let bound = remote::output(
        PREPARE_PROJECT,
        &[SETUP_MARKER, &id, &remote_path, &local_path],
    )?;
    let agent = choose_agent(&project, requested, bound.trim())?;

    tools::install(&config::instructions()?)?;
    mutagen::sync(&project)?;
    let mut secret_args = vec![remote_path.as_str()];
    secret_args.extend(secrets::names());
    remote::run(REMOVE_SECRETS, &secret_args)?;
    snapshot::take(&project.name)?;

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
        local_path.as_str(),
        image.as_str(),
        spec.as_str(),
        agent.name(),
    ];
    container_args.extend(mounts.iter().map(String::as_str));
    remote::run(ENSURE_CONTAINER, &container_args)?;

    mutagen::report_issues(&id)?;
    Ok(Attached { project, agent })
}

fn choose_agent(project: &Project, requested: Option<Agent>, bound: &str) -> Result<Agent> {
    match (requested, Agent::parse(bound)) {
        (Some(requested), Some(bound)) if requested != bound => bail!(
            "{} is bound to {bound}, switch it with `sai change {requested}`",
            project.name
        ),
        (requested, bound) => Ok(requested.or(bound).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::choose_agent;
    use crate::agent::Agent;
    use crate::project::Project;

    fn project() -> Project {
        Project {
            name: "app".to_owned(),
            local_path: PathBuf::from("/tmp/app"),
        }
    }

    #[test]
    fn uses_the_default_for_new_projects() {
        assert_eq!(choose_agent(&project(), None, "").unwrap(), Agent::Claude);
        assert_eq!(
            choose_agent(&project(), Some(Agent::Codex), "").unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn keeps_the_bound_agent() {
        assert_eq!(
            choose_agent(&project(), None, "codex").unwrap(),
            Agent::Codex
        );
        assert_eq!(
            choose_agent(&project(), Some(Agent::Codex), "codex").unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn refuses_a_second_agent() {
        let error = choose_agent(&project(), Some(Agent::Codex), "claude").unwrap_err();
        assert!(error.to_string().contains("sai change codex"));
    }
}
