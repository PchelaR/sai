use anyhow::Result;

use crate::project::Project;
use crate::{mutagen, output};

pub fn run(dir_path: &str) -> Result<()> {
    let project = Project::open(dir_path)?;
    output::step("Replacing VPS files with local versions");
    mutagen::keep_local(&project)?;
    mutagen::report_issues(&project.id())?;
    println!("Conflicts in {} resolved with local versions", project.name);
    Ok(())
}
