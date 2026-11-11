use anyhow::Result;

use crate::{mutagen, remote};

const LIST: &str = include_str!("scripts/list.sh");
const NOT_SYNCING: &str = "not syncing";

pub struct Row {
    pub name: String,
    pub agent: String,
    pub state: String,
    pub sync: String,
    pub path: String,
}

pub fn load() -> Result<Vec<Row>> {
    let output = remote::output(LIST, &[])?;
    let mut rows = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        let &[id, agent, state, path] = fields.as_slice() else {
            continue;
        };
        let sync = match mutagen::session(id)? {
            Some(session) => session.summary(),
            None => NOT_SYNCING.to_owned(),
        };
        rows.push(Row {
            name: id.strip_prefix("sai-").unwrap_or(id).to_owned(),
            agent: agent.to_owned(),
            state: state.to_owned(),
            sync,
            path: path.to_owned(),
        });
    }
    Ok(rows)
}

pub fn run() -> Result<()> {
    let rows = load()?;
    if rows.is_empty() {
        println!("No projects. Attach one with: sai attach <dir>");
        return Ok(());
    }
    print_row("NAME", "AGENT", "STATE", "SYNC", "PATH");
    for row in &rows {
        print_row(&row.name, &row.agent, &row.state, &row.sync, &row.path);
    }
    Ok(())
}

fn print_row(name: &str, agent: &str, state: &str, sync: &str, path: &str) {
    println!("{name:<20} {agent:<8} {state:<10} {sync:<28} {path}");
}
