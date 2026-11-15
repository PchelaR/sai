use std::collections::HashMap;

use anyhow::Result;

use super::Registry;
use crate::{mutagen, remote};

const LIST: &str = include_str!("scripts/list.sh");
const NOT_SYNCING: &str = "not syncing";
const NO_CONTAINER: &str = "-";
const ONLY_ON_VPS: &str = "only on VPS";

#[derive(Clone)]
pub struct FolderRow {
    pub name: String,
    pub path: String,
    pub sync: String,
    pub git: bool,
    pub skipped: usize,
}

impl FolderRow {
    pub fn label(&self) -> String {
        let mut label = self.name.clone();
        if self.git {
            label.push_str(" · git");
        }
        if self.skipped > 0 {
            label.push_str(&format!(" · {} skipped", self.skipped));
        }
        label
    }
}

pub struct Row {
    pub only_on_vps: bool,
    pub name: String,
    pub agent: String,
    pub state: String,
    pub sync: String,
    pub folders: Vec<FolderRow>,
}

pub fn load() -> Result<Vec<Row>> {
    let registry = Registry::load()?;
    let output = remote::output(LIST, &[])?;
    let (states, on_vps) = parse(&output);
    let syncs: HashMap<String, String> = mutagen::sessions()?
        .into_iter()
        .map(|session| (session.name.clone(), session.summary()))
        .collect();
    let mut rows = Vec::new();
    for project in registry.projects() {
        let mut folders = Vec::new();
        for folder in &project.folders {
            let sync = syncs
                .get(&project.target(folder).id)
                .cloned()
                .unwrap_or_else(|| NOT_SYNCING.to_owned());
            folders.push(FolderRow {
                name: folder.name.clone(),
                path: folder.path.display().to_string(),
                sync,
                git: folder.git,
                skipped: folder.skip.len(),
            });
        }
        rows.push(Row {
            only_on_vps: false,
            name: project.name.clone(),
            agent: project
                .agent
                .map(|agent| agent.to_string())
                .or_else(|| project.retired_agent.clone())
                .unwrap_or_default(),
            state: states
                .get(project.name.as_str())
                .unwrap_or(&NO_CONTAINER)
                .to_string(),
            sync: summary(&folders),
            folders,
        });
    }
    for name in on_vps {
        if registry.find(name).is_none() {
            rows.push(Row {
                only_on_vps: true,
                name: name.to_owned(),
                agent: String::new(),
                state: ONLY_ON_VPS.to_owned(),
                sync: "Enter: attach a folder".to_owned(),
                folders: Vec::new(),
            });
        }
    }
    Ok(rows)
}

fn parse(output: &str) -> (HashMap<&str, &str>, Vec<&str>) {
    let mut states = HashMap::new();
    let mut on_vps = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["container", name, state] => {
                states.insert(*name, *state);
            }
            ["files", name] => on_vps.push(*name),
            _ => {}
        }
    }
    (states, on_vps)
}

pub fn run() -> Result<()> {
    Registry::open()?;
    let rows = load()?;
    if rows.is_empty() {
        println!("No projects. Attach a folder with: sai attach <project> <folder>");
        return Ok(());
    }
    println!("{:<20} {:<8} {:<10} SYNC", "NAME", "AGENT", "STATE");
    let (orphans, rows): (Vec<&Row>, Vec<&Row>) = rows.iter().partition(|row| row.only_on_vps);
    for row in rows {
        println!(
            "{:<20} {:<8} {:<10} {}",
            row.name, row.agent, row.state, row.sync
        );
        for folder in &row.folders {
            println!(
                "  {:<18} {:<20} {}",
                folder.label(),
                folder.sync,
                folder.path
            );
        }
    }
    if !orphans.is_empty() {
        println!();
        println!("Only on the VPS, not attached on this computer:");
        for row in orphans {
            println!(
                "  {:<18} bring it back: sai attach {} <folder>, or delete it: sai rm {} --purge",
                row.name, row.name, row.name
            );
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
    use super::{FolderRow, parse, summary};

    #[test]
    fn reads_containers_and_project_files() {
        let (states, on_vps) = parse("container\tshop\trunning\nfiles\tshop\nfiles\told\njunk\n");
        assert_eq!(states.get("shop"), Some(&"running"));
        assert_eq!(on_vps, ["shop", "old"]);
    }

    fn folder(name: &str, sync: &str) -> FolderRow {
        FolderRow {
            name: name.to_owned(),
            path: String::new(),
            sync: sync.to_owned(),
            git: false,
            skipped: 0,
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
