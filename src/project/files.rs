use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};

use super::content::{self, Check, Selection};
use super::{Folder, Registry, detach};
use crate::chat::chats;
use crate::{attach, mutagen, output, remote, snapshot};

const REMOVE_SKIPPED: &str = include_str!("scripts/remove_skipped.sh");

pub fn run(name: Option<&str>, folder: Option<&str>) -> Result<()> {
    let mut registry = Registry::open()?;
    let project = registry.resolve(name)?.clone();
    let folder = match (folder, project.folders.as_slice()) {
        (Some(folder), _) => detach::find(&project, folder)?,
        (None, [only]) => only.clone(),
        (None, _) => bail!(
            "{} has several folders, name one: sai files {} <folder>. Its folders are: {}",
            project.name,
            project.name,
            project.folder_names().join(", ")
        ),
    };
    let Some(selection) = attach::ask_items(&folder.path, Some(folder.skip.clone()))? else {
        bail!(
            "nothing is selected, keep at least one item or detach the folder with `sai detach {} {}`",
            project.name,
            folder.name
        );
    };
    apply(&mut registry, &project.name, &folder, selection)
}

pub fn apply(
    registry: &mut Registry,
    name: &str,
    folder: &Folder,
    selection: Selection,
) -> Result<()> {
    let (uploaded, total) = (selection.uploaded, selection.total);
    if !change(registry, name, folder, selection)? {
        output::info("Nothing changed");
        return Ok(());
    }
    attach::ensure(registry, name, None)?;
    output::info(&format!(
        "{} now uploads {} of {}. Skipped items stay on your computer, the VPS copies were removed and stay in snapshots",
        folder.path.display(),
        super::size::human(uploaded),
        super::size::human(total)
    ));
    Ok(())
}

pub fn change(
    registry: &mut Registry,
    name: &str,
    folder: &Folder,
    selection: Selection,
) -> Result<bool> {
    let old: BTreeSet<String> = folder.skip.iter().cloned().collect();
    let new: BTreeSet<String> = selection.skip.iter().cloned().collect();
    if old == new {
        return Ok(false);
    }
    let _lock = super::lock::hold(name)?;
    chats::ensure_idle(name, registry.get(name)?.agent.unwrap_or_default())?;
    for path in &new {
        ensure!(is_safe(path), "cannot skip {path:?}");
    }
    attach::ensure_room(&folder.path, selection.uploaded)?;
    let project = registry.get(name)?.clone();
    let target = project.target(folder);
    mutagen::finish(&target.id)?;
    snapshot::take(name)?;
    // Repositories in folders that stop uploading lose their history on the VPS too,
    // before the folders are deleted there, or the git sync would bring them back
    let mut updated = project.clone();
    if let Some(changed) = updated
        .folders
        .iter_mut()
        .find(|changed| super::same_path(&changed.path, &folder.path))
    {
        changed.skip = selection.skip.clone();
    }
    attach::drop_stale_git(&updated, &updated.git_targets())?;
    let removed: Vec<&str> = new
        .iter()
        .filter(|path| content::check(path, &old) != Check::Off)
        .map(String::as_str)
        .collect();
    remove_remote(&target.remote, &removed)?;
    registry.set_skip(name, &folder.path, selection.skip)?;
    registry.save()?;
    Ok(true)
}

/// Deletes paths inside an uploaded folder on the VPS
pub fn remove_remote<S: AsRef<str>>(root: &str, paths: &[S]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec![root];
    for path in paths {
        ensure!(is_safe(path.as_ref()), "cannot remove {:?}", path.as_ref());
        args.push(path.as_ref());
    }
    remote::run(REMOVE_SKIPPED, &args)
}

fn is_safe(path: &str) -> bool {
    !path.is_empty()
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::is_safe;

    #[test]
    fn refuses_paths_that_leave_the_folder() {
        assert!(is_safe("data/raw"));
        assert!(is_safe("my file.bin"));
        assert!(!is_safe(""));
        assert!(!is_safe("/etc"));
        assert!(!is_safe("../x"));
        assert!(!is_safe("a/./b"));
        assert!(!is_safe("a//b"));
    }
}
