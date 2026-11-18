use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{Folder, Project, Registry, folder_name, id, remote_path};
use crate::agent::Agent;
use crate::vps::USER;
use crate::{mutagen, output, remote};

const MIGRATE: &str = include_str!("scripts/migrate.sh");

pub fn migrate(registry: &mut Registry) -> Result<()> {
    let legacy: Vec<_> = mutagen::sessions()?
        .into_iter()
        .filter_map(|session| {
            let name = legacy_name(&session.name)?.to_owned();
            Some((name, session))
        })
        .filter(|(name, _)| registry.find(name).is_none())
        .collect();
    for (name, session) in legacy {
        let path = PathBuf::from(session.local_path());
        let folder = folder_name(&path)?;
        output::step(&format!(
            "Upgrading project {name} so that it can hold several folders"
        ));
        let snapshots = format!("/home/{USER}/snapshots/{name}");
        let args = |mode: &str| {
            vec![
                mode.to_owned(),
                id(&name),
                remote_path(&name),
                snapshots.clone(),
                folder.clone(),
            ]
        };
        let agent = remote::output(MIGRATE, &as_strs(&args("check")))?;
        mutagen::finish(&session.name)
            .with_context(|| format!("cannot upgrade {name} while its sync is not connected"))?;
        let label = agent.trim();
        registry.insert(Project {
            name: name.clone(),
            agent: Agent::parse(label),
            retired_agent: (!label.is_empty() && Agent::parse(label).is_none())
                .then(|| label.to_owned()),
            vps: None,
            folders: vec![Folder {
                name: folder.clone(),
                path,
                git: false,
                skip: Vec::new(),
                history: Vec::new(),
            }],
        });
        registry.save()?;
        remote::run(MIGRATE, &as_strs(&args("move")))?;
        output::info(&format!(
            "{name} now has the folder {folder}, the agent sees it as /workspace/{folder}"
        ));
    }
    Ok(())
}

fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

fn legacy_name(session: &str) -> Option<&str> {
    session
        .strip_prefix("sai-")
        .filter(|name| !name.is_empty() && !name.contains("--"))
}

#[cfg(test)]
mod tests {
    use super::legacy_name;

    #[test]
    fn recognizes_single_folder_sessions() {
        assert_eq!(legacy_name("sai-shop"), Some("shop"));
        assert_eq!(legacy_name("sai-shop--front"), None);
        assert_eq!(legacy_name("other"), None);
    }
}
