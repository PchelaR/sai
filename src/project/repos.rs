use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::slug;
use crate::ignores;

pub const GIT_DIR: &str = ".git";
pub const LIMIT: usize = 20;
const MAX_DEPTH: usize = 4;
const MAX_SLUG: usize = 24;

pub fn find(root: &Path) -> Vec<PathBuf> {
    ignores::dirs(root, MAX_DEPTH)
        .into_iter()
        .filter(|relative| root.join(relative).join(GIT_DIR).is_dir())
        .collect()
}

pub fn label(relative: &Path) -> String {
    if relative.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Where the repository's .git lies inside the folder on the VPS
pub fn git_dir(relative: &Path) -> String {
    match label(relative).as_str() {
        "." => GIT_DIR.to_owned(),
        label => format!("{label}/{GIT_DIR}"),
    }
}

pub fn suffix(relative: &Path) -> String {
    if relative.as_os_str().is_empty() {
        return String::new();
    }
    let label = label(relative);
    let hash: String = Sha256::digest(label.as_bytes())
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut name = slug(&label);
    name.truncate(MAX_SLUG);
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        format!("--{hash}")
    } else {
        format!("--{name}-{hash}")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{find, git_dir, label, suffix};

    #[test]
    fn places_git_dirs_inside_the_folder() {
        assert_eq!(git_dir(Path::new("")), ".git");
        assert_eq!(
            git_dir(&PathBuf::from("libs").join("core")),
            "libs/core/.git"
        );
    }

    #[test]
    fn finds_nested_repositories() {
        let root = std::env::temp_dir().join(format!("sai-repos-{}", std::process::id()));
        for dir in [
            ".git",
            "tionix/.git",
            "libs/core/.git",
            "node_modules/pkg/.git",
            ".hidden/x/.git",
            "a/b/c/d/e/.git",
            "plain/src",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        let found: Vec<String> = find(&root).iter().map(|path| label(path)).collect();
        assert_eq!(found, [".", "tionix", "libs/core"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_sessions_of_nested_repositories() {
        assert_eq!(suffix(Path::new("")), "");
        let nested = suffix(&PathBuf::from("libs").join("Core_Lib"));
        assert!(nested.starts_with("--libs-core-lib-"));
        assert_ne!(nested, suffix(Path::new("libs-core-lib")));
    }
}
