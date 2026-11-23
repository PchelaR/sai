use std::collections::HashMap;

use anyhow::Result;

use super::{Project, Registry};
use crate::{mutagen, remote};

const LIST: &str = concat!(
    include_str!("../chat/agent_state.sh"),
    include_str!("scripts/list.sh")
);
const STATES: &str = concat!(
    include_str!("../chat/agent_state.sh"),
    include_str!("scripts/states.sh")
);
const NOT_SYNCING: &str = "not syncing";
const NO_CONTAINER: &str = "-";
const ONLY_ON_VPS: &str = "only on VPS";
const OTHER_VPS: &str = "other VPS";

#[derive(Clone)]
pub struct FolderRow {
    pub name: String,
    pub path: String,
    pub sync: String,
    pub git: bool,
    pub skipped: usize,
}

impl Row {
    /// The row shows a project of this VPS, so its agent state can be updated in place
    pub fn live(&self) -> bool {
        !self.only_on_vps && self.state != OTHER_VPS
    }

    pub fn set_agent_state(&mut self, state: &str) {
        self.state = describe_state(state);
    }
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
    pub review: usize,
    pub folders: Vec<FolderRow>,
}

struct Listing<'a> {
    vps: Option<&'a str>,
    states: HashMap<&'a str, &'a str>,
    on_vps: Vec<&'a str>,
    reviews: HashMap<&'a str, usize>,
}

pub fn load() -> Result<Vec<Row>> {
    let registry = Registry::load()?;
    let output = remote::output(LIST, &[])?;
    let listing = parse(&output);
    let syncs: HashMap<String, String> = mutagen::sessions()?
        .into_iter()
        .map(|session| (session.name.clone(), session.summary()))
        .collect();
    let mut rows = Vec::new();
    for project in registry.projects() {
        if on_other_vps(project, &listing)? {
            for id in project.live_sessions()? {
                mutagen::drop_if_present(&id)?;
            }
            rows.push(Row {
                only_on_vps: false,
                name: project.name.clone(),
                agent: project
                    .agent
                    .map(|agent| agent.to_string())
                    .unwrap_or_default(),
                state: OTHER_VPS.to_owned(),
                sync: "uploaded here on the next chat".to_owned(),
                review: 0,
                folders: project
                    .folders
                    .iter()
                    .map(|folder| FolderRow {
                        name: folder.name.clone(),
                        path: folder.path.display().to_string(),
                        sync: NOT_SYNCING.to_owned(),
                        git: folder.git,
                        skipped: folder.skip.len(),
                    })
                    .collect(),
            });
            continue;
        }
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
            state: describe_state(
                listing
                    .states
                    .get(project.name.as_str())
                    .unwrap_or(&NO_CONTAINER),
            ),
            sync: summary(&folders),
            review: listing
                .reviews
                .get(project.name.as_str())
                .copied()
                .unwrap_or_default(),
            folders,
        });
    }
    for name in listing.on_vps {
        if registry.find(name).is_none() {
            rows.push(Row {
                only_on_vps: true,
                name: name.to_owned(),
                agent: String::new(),
                state: ONLY_ON_VPS.to_owned(),
                sync: "Enter: attach a folder".to_owned(),
                review: 0,
                folders: Vec::new(),
            });
        }
    }
    Ok(rows)
}

/// What the agents of the running projects are doing now, a quick check for the menu
pub fn agent_states() -> Option<HashMap<String, String>> {
    let output = remote::probe(STATES, &[])?;
    Some(
        parse(&output)
            .states
            .into_iter()
            .map(|(name, state)| (name.to_owned(), state.to_owned()))
            .collect(),
    )
}

/// The agent stopped and waits for the user: it finished a task or asks for permission
pub fn needs_user(before: &str, now: &str) -> bool {
    (before == "working" && now == "waiting") || (before != "asking" && now == "asking")
}

fn parse(output: &str) -> Listing<'_> {
    let mut listing = Listing {
        vps: None,
        states: HashMap::new(),
        on_vps: Vec::new(),
        reviews: HashMap::new(),
    };
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["container", name, state] => {
                listing.states.insert(*name, *state);
            }
            ["files", name] => listing.on_vps.push(*name),
            ["vps", id] => listing.vps = Some(id.trim()),
            ["review", name, count] => {
                listing
                    .reviews
                    .insert(*name, count.trim().parse().unwrap_or_default());
            }
            _ => {}
        }
    }
    listing
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
        if row.review > 0 {
            println!(
                "  {} file(s) that can run code on your computer changed, see: sai review {}",
                row.review, row.name
            );
        }
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

fn on_other_vps(project: &Project, listing: &Listing) -> Result<bool> {
    let Some(vps) = listing.vps else {
        return Ok(false);
    };
    Ok(match &project.vps {
        Some(own) => own != vps,
        None => {
            !listing.on_vps.contains(&project.name.as_str()) && !project.live_sessions()?.is_empty()
        }
    })
}

fn describe_state(state: &str) -> String {
    match state {
        "working" => "working",
        "asking" => "asks you",
        "waiting" => "your turn",
        "idle" => "ready",
        other => other,
    }
    .to_owned()
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
    use super::{FolderRow, needs_user, parse, summary};

    #[test]
    fn calls_the_user_when_the_agent_stops() {
        assert!(needs_user("working", "waiting"));
        assert!(needs_user("working", "asking"));
        assert!(needs_user("waiting", "asking"));
        assert!(!needs_user("asking", "asking"));
        assert!(!needs_user("asking", "working"));
        assert!(!needs_user("waiting", "waiting"));
        assert!(!needs_user("idle", "waiting"));
    }

    #[test]
    fn reads_containers_and_project_files() {
        let listing =
            parse("container\tshop\trunning\nfiles\tshop\nfiles\told\nreview\tshop\t2\njunk\n");
        assert_eq!(listing.states.get("shop"), Some(&"running"));
        assert_eq!(listing.on_vps, ["shop", "old"]);
        assert_eq!(listing.reviews.get("shop"), Some(&2));
        assert_eq!(parse("vps\tabc123\n").vps, Some("abc123"));
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
