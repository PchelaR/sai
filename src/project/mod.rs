pub mod change;
pub mod detach;
mod legacy;
pub mod list;
mod registry;
pub mod remove;
pub mod rename;
pub mod size;
pub mod stop;

use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, Result, ensure};

use crate::agent::Agent;
use crate::vps::USER;

pub use registry::{Registry, same_path};

const MAX_NAME_LENGTH: usize = 40;

#[derive(Clone)]
pub struct Folder {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct Project {
    pub name: String,
    pub agent: Option<Agent>,
    pub folders: Vec<Folder>,
}

pub struct SyncTarget {
    pub id: String,
    pub local: PathBuf,
    pub remote: String,
}

impl Project {
    pub fn id(&self) -> String {
        id(&self.name)
    }

    pub fn remote_path(&self) -> String {
        remote_path(&self.name)
    }

    pub fn targets(&self) -> Vec<SyncTarget> {
        self.folders
            .iter()
            .map(|folder| self.target(folder))
            .collect()
    }

    pub fn target(&self, folder: &Folder) -> SyncTarget {
        SyncTarget {
            id: sync_id(&self.name, &folder.name),
            local: folder.path.clone(),
            remote: format!("{}/{}", self.remote_path(), folder.name),
        }
    }

    pub fn folder_names(&self) -> Vec<&str> {
        self.folders
            .iter()
            .map(|folder| folder.name.as_str())
            .collect()
    }
}

pub fn id(name: &str) -> String {
    format!("sai-{name}")
}

pub fn sync_id(project: &str, folder: &str) -> String {
    format!("{}--{folder}", id(project))
}

pub fn remote_path(name: &str) -> String {
    format!("/home/{USER}/projects/{name}")
}

pub fn local_path(dir_path: &str) -> Result<PathBuf> {
    let path = fs::canonicalize(dir_path)
        .map(without_verbatim_prefix)
        .with_context(|| format!("directory not found: {dir_path}"))?;
    ensure!(path.is_dir(), "not a directory: {}", path.display());
    Ok(path)
}

pub fn current_dir() -> Result<PathBuf> {
    local_path(&env::current_dir()?.display().to_string())
}

pub fn folder_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .map(|name| slug(&name.to_string_lossy()))
        .unwrap_or_default();
    ensure!(
        !name.is_empty(),
        "cannot derive a folder name from {}, give it one with --as",
        path.display()
    );
    Ok(name)
}

pub fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name.len() <= MAX_NAME_LENGTH && slug(name) == name,
        "invalid name {name:?}, use up to {MAX_NAME_LENGTH} lowercase latin letters, digits and single dashes"
    );
    Ok(())
}

pub fn slug(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let stripped = {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            Some(PathBuf::from(format!(r"\\{rest}")))
        } else {
            text.strip_prefix(r"\\?\")
                .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
                .map(PathBuf::from)
        }
    };
    stripped.unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{id, remote_path, slug, sync_id, validate_name, without_verbatim_prefix};

    #[test]
    fn strips_windows_verbatim_prefix() {
        let strip = |path: &str| without_verbatim_prefix(PathBuf::from(path));
        assert_eq!(strip(r"\\?\W:\Dev\app"), PathBuf::from(r"W:\Dev\app"));
        assert_eq!(
            strip(r"\\?\UNC\server\share\app"),
            PathBuf::from(r"\\server\share\app")
        );
        assert_eq!(strip("/home/user/app"), PathBuf::from("/home/user/app"));
    }

    #[test]
    fn slugs_folder_names() {
        assert_eq!(slug("My Project"), "my-project");
        assert_eq!(slug("sai_test.v2"), "sai-test-v2");
        assert_eq!(slug("--weird__"), "weird");
        assert_eq!(slug("проект"), "");
    }

    #[test]
    fn validates_names() {
        assert!(validate_name("sai-test").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("Sai").is_err());
        assert!(validate_name("../x").is_err());
        assert!(validate_name("a--b").is_err());
        assert!(validate_name(&"a".repeat(41)).is_err());
    }

    #[test]
    fn derives_remote_names() {
        assert_eq!(id("app"), "sai-app");
        assert_eq!(sync_id("app", "front"), "sai-app--front");
        assert_eq!(remote_path("app"), "/home/sai/projects/app");
    }
}
