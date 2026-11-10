use anyhow::Result;

use crate::{mutagen, remote};

const LIST: &str = include_str!("scripts/list.sh");
const NOT_SYNCING: &str = "not syncing";

pub fn run() -> Result<()> {
    let output = remote::output(LIST, &[])?;
    let projects: Vec<Vec<&str>> = output
        .lines()
        .map(|line| line.splitn(3, '\t').collect())
        .collect();
    if projects.is_empty() {
        println!("No projects. Attach one with: sai attach <dir>");
        return Ok(());
    }

    print_row("NAME", "AGENT", "SYNC", "PATH");
    for project in projects {
        let &[id, state, path] = project.as_slice() else {
            continue;
        };
        let sync = match mutagen::session(id)? {
            Some(session) => session.summary(),
            None => NOT_SYNCING.to_owned(),
        };
        let name = id.strip_prefix("sai-").unwrap_or(id);
        print_row(name, state, &sync, path);
    }
    Ok(())
}

fn print_row(name: &str, state: &str, sync: &str, path: &str) {
    println!("{name:<20} {state:<10} {sync:<28} {path}");
}
