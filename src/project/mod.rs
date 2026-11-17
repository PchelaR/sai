pub mod change;
pub mod content;
pub mod detach;
pub mod files;
pub mod folders;
mod legacy;
pub mod list;
pub mod lock;
mod registry;
pub mod remove;
pub mod rename;
pub mod repos;
pub mod size;
pub mod stop;
pub mod suggest;

use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, Result, bail, ensure};

use crate::agent::Agent;
use crate::vps::USER;

pub use registry::{Registry, is_inside, same_path};

const MAX_NAME_LENGTH: usize = 40;
const GIT_SUFFIX: &str = "--git";

#[derive(Clone)]
pub struct Folder {
    pub name: String,
    pub path: PathBuf,
    pub git: bool,
    pub skip: Vec<String>,
}

#[derive(Clone)]
pub struct Project {
    pub name: String,
    pub agent: Option<Agent>,
    pub retired_agent: Option<String>,
    pub vps: Option<String>,
    pub folders: Vec<Folder>,
}

pub struct SyncTarget {
    pub id: String,
    pub local: PathBuf,
    pub remote: String,
    pub skip: Vec<String>,
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
            skip: folder.skip.clone(),
        }
    }

    pub fn git_targets(&self) -> Vec<SyncTarget> {
        self.folders
            .iter()
            .filter(|folder| folder.git)
            .flat_map(|folder| {
                repos::find(&folder.path)
                    .into_iter()
                    .take(repos::LIMIT)
                    .map(|relative| self.git_target(folder, &relative))
            })
            .collect()
    }

    pub fn git_target(&self, folder: &Folder, relative: &Path) -> SyncTarget {
        let target = self.target(folder);
        let remote = match repos::label(relative).as_str() {
            "." => target.remote,
            label => format!("{}/{label}", target.remote),
        };
        SyncTarget {
            id: format!("{}{}", self.git_prefix(folder), repos::suffix(relative)),
            local: target.local.join(relative).join(repos::GIT_DIR),
            remote: format!("{remote}/{}", repos::GIT_DIR),
            skip: Vec::new(),
        }
    }

    pub fn git_prefix(&self, folder: &Folder) -> String {
        format!("{}{GIT_SUFFIX}", sync_id(&self.name, &folder.name))
    }

    pub fn git_session_ids(&self, folder: &Folder) -> Result<Vec<String>> {
        let prefix = self.git_prefix(folder);
        Ok(crate::mutagen::session_names()?
            .into_iter()
            .filter(|name| {
                name.strip_prefix(&prefix)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with("--"))
            })
            .collect())
    }

    pub fn live_sessions(&self) -> Result<Vec<String>> {
        let names = crate::mutagen::session_names()?;
        let targets: Vec<String> = self.targets().into_iter().map(|target| target.id).collect();
        let prefixes: Vec<String> = self
            .folders
            .iter()
            .map(|folder| self.git_prefix(folder))
            .collect();
        Ok(names
            .into_iter()
            .filter(|name| {
                targets.contains(name)
                    || prefixes.iter().any(|prefix| {
                        name.strip_prefix(prefix.as_str())
                            .is_some_and(|rest| rest.is_empty() || rest.starts_with("--"))
                    })
            })
            .collect())
    }

    pub fn session_ids(&self) -> Result<Vec<String>> {
        let mut ids: Vec<String> = self.targets().into_iter().map(|target| target.id).collect();
        for folder in &self.folders {
            ids.extend(self.git_session_ids(folder)?);
        }
        Ok(ids)
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

pub fn ensure_attachable(path: &Path) -> Result<()> {
    let path = fs::canonicalize(path)
        .map(without_verbatim_prefix)
        .unwrap_or_else(|_| path.to_path_buf());
    let home = etcetera::home_dir().ok();
    ensure!(
        path.parent().is_some() && home.is_none_or(|home| !same_path(&home, &path)),
        "{} is your home folder or a whole disk, attach the folder of a single project instead",
        path.display()
    );
    if let Some(name) = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .find(|name| content::hidden(name, true))
    {
        bail!(
            "{} is inside {name}, which sai never uploads: it holds secrets, build output or dependencies",
            path.display()
        );
    }
    Ok(())
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

pub fn name_from(input: &str) -> Result<String> {
    let name = slug(input.trim());
    ensure!(
        !name.is_empty() && name.len() <= MAX_NAME_LENGTH,
        "cannot make a name from {input:?}, use up to {MAX_NAME_LENGTH} latin letters and digits"
    );
    Ok(name)
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
        assert_eq!(
            super::name_from("my_test_project_").unwrap(),
            "my-test-project"
        );
        assert_eq!(super::name_from(" My Shop ").unwrap(), "my-shop");
        assert!(super::name_from("___").is_err());
        assert!(super::ensure_attachable(std::path::Path::new("/home/x/.ssh")).is_err());
        assert!(
            super::ensure_attachable(std::path::Path::new("/code/app/node_modules/x")).is_err()
        );
        assert!(super::ensure_attachable(std::path::Path::new("/code/app")).is_ok());
    }

    #[test]
    fn derives_remote_names() {
        assert_eq!(id("app"), "sai-app");
        assert_eq!(sync_id("app", "front"), "sai-app--front");
        assert_eq!(remote_path("app"), "/home/sai/projects/app");
    }
}
