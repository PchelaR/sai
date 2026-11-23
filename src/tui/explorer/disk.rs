//! Folders on the disk: listing them, the roots and drives to start from
use super::*;

pub(super) fn full_path(target: &Target) -> PathBuf {
    match target {
        Target::Outside(path) | Target::Root(path) => path.clone(),
        Target::Inside(root, path) => root.join(path),
    }
}

pub(super) fn folder_name(path: &Path) -> String {
    project::folder_name(path).unwrap_or_else(|_| "folder".to_owned())
}

pub(super) fn unique(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|existing| existing == name) {
        return name.to_owned();
    }
    (2..)
        .map(|number| format!("{name}-{number}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_owned())
}

pub(super) fn list_dirs(path: &Path) -> Vec<Dir> {
    let mut dirs: Vec<Dir> = fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    !name.starts_with('.')
                        && !content::hidden(&name, true)
                        && !hidden_by_system(entry)
                })
                .map(|entry| {
                    let path = entry.path();
                    Dir {
                        git: path.join(repos::GIT_DIR).is_dir(),
                        path,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    dirs.sort_by_key(|dir| {
        dir.path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    dirs
}

#[cfg(windows)]
pub(super) fn hidden_by_system(entry: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    entry
        .metadata()
        .is_ok_and(|metadata| metadata.file_attributes() & (HIDDEN | SYSTEM) != 0)
}

#[cfg(not(windows))]
pub(super) fn hidden_by_system(_: &fs::DirEntry) -> bool {
    false
}

pub(super) fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = etcetera::home_dir().into_iter().collect();
    if cfg!(windows) {
        roots.extend(drives());
    } else {
        roots.push(PathBuf::from("/"));
    }
    roots
}

pub(super) fn drives() -> Vec<PathBuf> {
    let (sender, receiver) = mpsc::channel();
    for letter in 'C'..='Z' {
        let sender = sender.clone();
        thread::spawn(move || {
            let drive = PathBuf::from(format!("{letter}:\\"));
            if drive.is_dir() {
                sender.send(drive).ok();
            }
        });
    }
    drop(sender);
    let deadline = Instant::now() + DRIVE_WAIT;
    let mut found = Vec::new();
    while let Ok(drive) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        found.push(drive);
    }
    found.sort();
    found
}

pub(super) fn display_root(path: &Path) -> String {
    match etcetera::home_dir() {
        Ok(home) if project::same_path(&home, path) => "~".to_owned(),
        _ => path.display().to_string(),
    }
}
