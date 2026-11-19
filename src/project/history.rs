use std::io::IsTerminal;

use anyhow::{Result, ensure};
use dialoguer::MultiSelect;

use super::{Registry, lock, repos};
use crate::output;

/// Lets the user pick which repositories give their history to the agent, for folders
/// with more than repos::LIMIT of them
pub fn run(name: Option<&str>) -> Result<()> {
    let mut registry = Registry::open()?;
    let project = registry.resolve(name)?.clone();
    let git: Vec<_> = project.folders.iter().filter(|folder| folder.git).collect();
    if git.is_empty() {
        output::info(&format!(
            "The agent of {} gets no git history. Turn it on for a folder with: sai attach {} <folder>",
            project.name, project.name
        ));
        return Ok(());
    }
    let crowded: Vec<_> = git
        .into_iter()
        .filter(|folder| folder.repos().len() > repos::LIMIT)
        .collect();
    if crowded.is_empty() {
        output::info(&format!(
            "Every folder of {} has no more than {} git repositories, the agent gets the history of all of them",
            project.name,
            repos::LIMIT
        ));
        return Ok(());
    }
    ensure!(
        std::io::stdin().is_terminal(),
        "choosing repositories needs a terminal, run `sai history {}` in one",
        project.name
    );
    let _lock = lock::hold(&project.name)?;
    for folder in crowded {
        let labels: Vec<String> = folder
            .repos()
            .iter()
            .map(|relative| repos::label(relative))
            .collect();
        let chosen: Vec<String> = folder
            .history_repos()
            .iter()
            .map(|relative| repos::label(relative))
            .collect();
        let mut defaults: Vec<bool> = labels.iter().map(|label| chosen.contains(label)).collect();
        let picked = loop {
            let Some(picked) = MultiSelect::new()
                .with_prompt(format!(
                    "{}: pick up to {} repositories whose history the agent gets (Space toggles, Enter saves, Esc cancels)",
                    folder.path.display(),
                    repos::LIMIT
                ))
                .items(&labels)
                .defaults(&defaults)
                .interact_opt()?
            else {
                output::info("Nothing was changed");
                return Ok(());
            };
            if (1..=repos::LIMIT).contains(&picked.len()) {
                break picked;
            }
            output::warning(&format!(
                "picked {}, pick from 1 to {} repositories",
                picked.len(),
                repos::LIMIT
            ));
            defaults = (0..labels.len())
                .map(|index| picked.contains(&index))
                .collect();
        };
        let history = picked
            .into_iter()
            .map(|index| labels[index].clone())
            .collect();
        registry.set_history(&project.name, &folder.path, history)?;
    }
    registry.save()?;
    output::info(&format!(
        "Saved. The agent gets the chosen history on the next `sai chat {}`",
        project.name
    ));
    Ok(())
}
