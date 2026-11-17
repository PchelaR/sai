use std::cell::Cell;
use std::path::PathBuf;

use anyhow::Result;

use super::content::Selection;
use super::{Registry, detach, files};
use crate::agent::Agent;
use crate::chat::chats;
use crate::{attach, output};

pub struct Addition {
    pub path: PathBuf,
    pub alias: String,
    pub selection: Selection,
}

pub struct Change {
    pub path: PathBuf,
    pub selection: Selection,
}

#[derive(Default)]
pub struct Plan {
    pub add: Vec<Addition>,
    pub remove: Vec<PathBuf>,
    pub change: Vec<Change>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty() && self.change.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.add.is_empty() {
            parts.push(format!(
                "Attach {}.",
                names(self.add.iter().map(|item| &item.path))
            ));
        }
        if !self.remove.is_empty() {
            parts.push(format!(
                "Detach {}: local files stay, the VPS copy is removed and stays in snapshots.",
                names(self.remove.iter())
            ));
        }
        if !self.change.is_empty() {
            parts.push(format!(
                "Change what is uploaded from {}.",
                names(self.change.iter().map(|item| &item.path))
            ));
        }
        parts.join(" ")
    }
}

fn names<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> String {
    paths
        .map(|path| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn apply(project: &str, agent: Option<Agent>, plan: Plan, git: bool) -> Result<()> {
    let _lock = super::lock::hold(project)?;
    if !plan.remove.is_empty() || !plan.change.is_empty() {
        let bound = Registry::open()?.get(project)?.agent.unwrap_or_default();
        chats::ensure_idle(project, bound)?;
    }
    for path in &plan.remove {
        detach::run(project, &path.display().to_string())?;
    }
    let mut registry = Registry::open()?;
    let mut changed = false;
    for item in plan.change {
        let found = registry.get(project)?.clone();
        let folder = detach::find(&found, &item.path.display().to_string())?;
        changed |= files::change(&mut registry, project, &folder, item.selection)?;
    }
    for item in plan.add {
        let selection = Cell::new(Some(item.selection));
        changed |= attach::register(
            &mut registry,
            project,
            &item.path.display().to_string(),
            agent,
            Some(&item.alias),
            &move |_| Ok(selection.take()),
            &move |_, _| Ok(git),
        )?
        .is_some();
    }
    if changed {
        attach::ensure(&mut registry, project, agent)?;
    }
    if registry.get(project)?.folders.is_empty() {
        return Ok(());
    }
    output::info(&format!(
        "{project} is up to date. Talk to the agent with: sai chat {project}"
    ));
    Ok(())
}
