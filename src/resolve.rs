use anyhow::Result;

use crate::project::Registry;
use crate::{mutagen, output};

pub fn run(name: Option<&str>) -> Result<()> {
    let registry = Registry::open()?;
    let project = registry.resolve(name)?;
    for target in project.targets() {
        // Folders the agent deleted are not conflicts to undo
        mutagen::finish_deletions(&target)?;
        output::step(&format!(
            "Replacing VPS files with local versions in {}",
            target.local.display()
        ));
        mutagen::keep_local(&target)?;
        mutagen::report_issues(&target.id)?;
    }
    output::info(&format!(
        "Conflicts in {} resolved with local versions",
        project.name
    ));
    Ok(())
}
