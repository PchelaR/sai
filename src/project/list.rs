use std::collections::HashMap;

use anyhow::Result;

use super::Registry;
use crate::{mutagen, remote};

const LIST: &str = include_str!("scripts/list.sh");
const NOT_SYNCING: &str = "not syncing";
const NO_CONTAINER: &str = "-";

pub struct FolderRow {
    pub name: String,
    pub path: String,
    pub sync: String,
}

pub struct Row {
    pub name: String,
    pub agent: String,
    pub state: String,
    pub sync: String,
    pub folders: Vec<FolderRow>,
}

pub fn load() -> Result<Vec<Row>> {
    let registry = Registry::load()?;
    let output = remote::output(LIST, &[])?;
    let states: HashMap<&str, &str> = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let mut rows = Vec::new();
    for project in registry.projects() {
        let mut folders = Vec::new();
        for folder in &project.folders {
            let sync = match mutagen::session(&project.target(folder).id)? {
                Some(session) => session.summary(),
                None => NOT_SYNCING.to_owned(),
            };
            folders.push(FolderRow {
                name: folder.name.clone(),
                path: folder.path.display().to_string(),
                sync,
            });
        }
        rows.push(Row {
            name: project.name.clone(),
            agent: project
                .agent
                .map_or_else(String::new, |agent| agent.to_string()),
            state: states
                .get(project.name.as_str())
                .unwrap_or(&NO_CONTAINER)
                .to_string(),
            sync: summary(&folders),
            folders,
        });
    }
    Ok(rows)
}

pub fn run() -> Result<()> {
    Registry::open()?;
    let rows = load()?;
    if rows.is_empty() {
        println!("No projects. Attach a folder with: sai attach <project> <folder>");
        return Ok(());
    }
    println!("{:<20} {:<8} {:<10} SYNC", "NAME", "AGENT", "STATE");
    for row in &rows {
        println!(
            "{:<20} {:<8} {:<10} {}",
            row.name, row.agent, row.state, row.sync
        );
        for folder in &row.folders {
            println!("  {:<18} {:<20} {}", folder.name, folder.sync, folder.path);
        }
    }
    Ok(())
}

fn summary(folders: &[FolderRow]) -> String {
    let mut states: Vec<&str> = folders.iter().map(|folder| folder.sync.as_str()).collect();
    states.dedup();
    match states.as_slice() {
        [] => "no folders".to_owned(),
        [single] => (*single).to_owned(),
        _ => folders
            .iter()
            .map(|folder| format!("{}: {}", folder.name, folder.sync))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::{FolderRow, summary};

    fn folder(name: &str, sync: &str) -> FolderRow {
        FolderRow {
            name: name.to_owned(),
            path: String::new(),
            sync: sync.to_owned(),
        }
    }

    #[test]
    fn summarizes_folder_syncs() {
        assert_eq!(summary(&[]), "no folders");
        assert_eq!(
            summary(&[folder("a", "watching"), folder("b", "watching")]),
            "watching"
        );
        assert_eq!(
            summary(&[folder("a", "watching"), folder("b", "paused")]),
            "a: watching, b: paused"
        );
    }
}
