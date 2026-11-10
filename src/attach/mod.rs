use anyhow::Result;

use crate::agent::{self, Agent};
use crate::config::Config;
use crate::project::Project;
use crate::vps::SETUP_MARKER;
use crate::{fingerprint, mutagen, remote, secrets, snapshot, ssh, tools};

const PREPARE_PROJECT: &str = include_str!("scripts/prepare_project.sh");
const REMOVE_SECRETS: &str = include_str!("scripts/remove_secrets.sh");
const ENSURE_CONTAINER: &str = include_str!("scripts/ensure_container.sh");

pub fn run(dir_path: &str, agent: Agent) -> Result<()> {
    let project = ensure(dir_path, agent)?;
    println!(
        "{} is attached. Talk to the agent with: sai chat {dir_path}",
        project.name
    );
    Ok(())
}

pub fn ensure(dir_path: &str, agent: Agent) -> Result<Project> {
    ssh::install_host_alias(&Config::load()?)?;
    mutagen::ensure_installed()?;
    let image = agent.image()?;
    let project = Project::open(dir_path)?;
    let id = project.id();
    let local_path = project.local_path.display().to_string();
    let remote_path = project.remote_path();

    remote::run(
        PREPARE_PROJECT,
        &[SETUP_MARKER, &id, &remote_path, &local_path],
    )?;
    tools::install(&image)?;
    mutagen::sync(&project)?;
    let mut secret_args = vec![remote_path.as_str()];
    secret_args.extend(secrets::names());
    remote::run(REMOVE_SECRETS, &secret_args)?;
    snapshot::take(&project.name)?;
    remote::run(
        ENSURE_CONTAINER,
        &[
            &id,
            &remote_path,
            &local_path,
            &image.reference(),
            agent::SESSION,
            &fingerprint::of(ENSURE_CONTAINER),
            &tools::policy_path(&image.policy),
            image.policy.container_path,
        ],
    )?;
    mutagen::report_issues(&id)?;
    Ok(project)
}
