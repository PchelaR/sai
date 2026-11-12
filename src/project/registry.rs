use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::{Folder, Project, legacy, validate_name};
use crate::agent::Agent;
use crate::{config, files};

const FILE: &str = "projects.toml";

#[derive(Default, Deserialize, Serialize)]
struct Stored {
    #[serde(default)]
    projects: BTreeMap<String, StoredProject>,
}

#[derive(Default, Deserialize, Serialize)]
struct StoredProject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
    #[serde(default)]
    folders: Vec<StoredFolder>,
}

#[derive(Deserialize, Serialize)]
struct StoredFolder {
    name: String,
    path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    git: bool,
}

pub struct Registry {
    projects: BTreeMap<String, Project>,
}

impl Registry {
    pub fn open() -> Result<Self> {
        let mut registry = Self::load()?;
        legacy::migrate(&mut registry)?;
        Ok(registry)
    }

    pub fn load() -> Result<Self> {
        let path = path()?;
        let stored: Stored = toml::from_str(&files::read_or_empty(&path)?)
            .with_context(|| format!("invalid project list at {}", path.display()))?;
        let projects = stored
            .projects
            .into_iter()
            .map(|(name, project)| {
                let project = Project {
                    agent: project.agent.as_deref().and_then(Agent::parse),
                    folders: project
                        .folders
                        .into_iter()
                        .map(|folder| Folder {
                            name: folder.name,
                            path: PathBuf::from(folder.path),
                            git: folder.git,
                        })
                        .collect(),
                    name: name.clone(),
                };
                (name, project)
            })
            .collect();
        Ok(Self { projects })
    }

    pub fn save(&self) -> Result<()> {
        let stored = Stored {
            projects: self
                .projects
                .values()
                .map(|project| {
                    let stored = StoredProject {
                        agent: project.agent.map(|agent| agent.name().to_owned()),
                        folders: project
                            .folders
                            .iter()
                            .map(|folder| StoredFolder {
                                name: folder.name.clone(),
                                path: folder.path.display().to_string(),
                                git: folder.git,
                            })
                            .collect(),
                    };
                    (project.name.clone(), stored)
                })
                .collect(),
        };
        let path = path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, toml::to_string(&stored)?)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values()
    }

    pub fn find(&self, name: &str) -> Option<&Project> {
        self.projects.get(name)
    }

    pub fn get(&self, name: &str) -> Result<&Project> {
        self.find(name)
            .with_context(|| format!("no project named {name}, see `sai list`"))
    }

    pub fn resolve(&self, name: Option<&str>) -> Result<&Project> {
        if let Some(name) = name {
            return self.get(name);
        }
        let current = super::current_dir()?;
        self.projects
            .values()
            .find(|project| {
                project
                    .folders
                    .iter()
                    .any(|folder| is_inside(&current, &folder.path))
            })
            .with_context(|| {
                format!(
                    "{} is not attached to any project, attach it with: sai attach <project> .",
                    current.display()
                )
            })
    }

    pub fn create(&mut self, name: &str, agent: Agent) -> Result<()> {
        validate_name(name)?;
        ensure!(
            !self.projects.contains_key(name),
            "project {name} already exists"
        );
        self.projects.insert(
            name.to_owned(),
            Project {
                name: name.to_owned(),
                agent: Some(agent),
                folders: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn add_folder(&mut self, project: &str, folder: Folder) -> Result<()> {
        validate_name(&folder.name)?;
        for other in self.projects.values() {
            for existing in &other.folders {
                if same_path(&existing.path, &folder.path) {
                    bail!(
                        "{} is already attached to project {} as {}",
                        folder.path.display(),
                        other.name,
                        existing.name
                    );
                }
                if is_inside(&folder.path, &existing.path)
                    || is_inside(&existing.path, &folder.path)
                {
                    bail!(
                        "{} overlaps with {} attached to project {}, attach folders that do not contain each other",
                        folder.path.display(),
                        existing.path.display(),
                        other.name
                    );
                }
            }
        }
        let target = self
            .projects
            .get_mut(project)
            .with_context(|| format!("no project named {project}"))?;
        ensure!(
            target
                .folders
                .iter()
                .all(|existing| existing.name != folder.name),
            "project {project} already has a folder named {}, pick another name with --as",
            folder.name
        );
        target.folders.push(folder);
        Ok(())
    }

    pub fn remove_folder(&mut self, project: &str, path: &Path) -> Result<Folder> {
        let target = self
            .projects
            .get_mut(project)
            .with_context(|| format!("no project named {project}"))?;
        let index = target
            .folders
            .iter()
            .position(|folder| same_path(&folder.path, path))
            .with_context(|| format!("{} is not attached to {project}", path.display()))?;
        Ok(target.folders.remove(index))
    }

    pub fn set_git(&mut self, project: &str, path: &Path, git: bool) -> Result<()> {
        let target = self
            .projects
            .get_mut(project)
            .with_context(|| format!("no project named {project}"))?;
        let folder = target
            .folders
            .iter_mut()
            .find(|folder| same_path(&folder.path, path))
            .with_context(|| format!("{} is not attached to {project}", path.display()))?;
        folder.git = git;
        Ok(())
    }

    pub fn set_agent(&mut self, project: &str, agent: Agent) -> Result<()> {
        self.projects
            .get_mut(project)
            .with_context(|| format!("no project named {project}"))?
            .agent = Some(agent);
        Ok(())
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        validate_name(new)?;
        ensure!(
            !self.projects.contains_key(new),
            "project {new} already exists"
        );
        let mut project = self
            .projects
            .remove(old)
            .with_context(|| format!("no project named {old}"))?;
        project.name = new.to_owned();
        self.projects.insert(new.to_owned(), project);
        Ok(())
    }

    pub fn insert(&mut self, project: Project) {
        self.projects.insert(project.name.clone(), project);
    }

    pub fn remove(&mut self, name: &str) -> Option<Project> {
        self.projects.remove(name)
    }
}

fn path() -> Result<PathBuf> {
    Ok(config::dir()?.join(FILE))
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    normalized(left) == normalized(right)
}

fn is_inside(path: &Path, folder: &Path) -> bool {
    let path = normalized(path);
    let folder = normalized(folder);
    path.len() > folder.len() && path.starts_with(&folder)
}

fn normalized(path: &Path) -> Vec<String> {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| {
            let text = component.as_os_str().to_string_lossy();
            if cfg!(windows) {
                text.to_lowercase()
            } else {
                text.into_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use super::{Registry, is_inside, same_path as same};
    use crate::agent::Agent;
    use crate::project::Folder;

    fn folder(name: &str, path: &str) -> Folder {
        Folder {
            name: name.to_owned(),
            path: PathBuf::from(path),
            git: false,
        }
    }

    fn registry() -> Registry {
        let mut registry = Registry {
            projects: BTreeMap::new(),
        };
        registry.create("shop", Agent::Claude).unwrap();
        registry
            .add_folder("shop", folder("front", "/code/front"))
            .unwrap();
        registry.create("blog", Agent::Codex).unwrap();
        registry
    }

    #[test]
    fn compares_whole_path_components() {
        assert!(same(Path::new("/code/front"), Path::new("/code/front/")));
        assert!(is_inside(
            Path::new("/code/front/src"),
            Path::new("/code/front")
        ));
        assert!(!is_inside(
            Path::new("/code/frontend"),
            Path::new("/code/front")
        ));
        assert!(!is_inside(
            Path::new("/code/front"),
            Path::new("/code/front")
        ));
    }

    #[test]
    fn a_folder_belongs_to_one_project() {
        let mut registry = registry();
        let error = registry
            .add_folder("blog", folder("front", "/code/front"))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("already attached to project shop")
        );
    }

    #[test]
    fn folders_cannot_contain_each_other() {
        let mut registry = registry();
        assert!(
            registry
                .add_folder("blog", folder("src", "/code/front/src"))
                .is_err()
        );
        assert!(
            registry
                .add_folder("blog", folder("code", "/code"))
                .is_err()
        );
        assert!(
            registry
                .add_folder("blog", folder("back", "/code/back"))
                .is_ok()
        );
    }

    #[test]
    fn folder_names_are_unique_within_a_project() {
        let mut registry = registry();
        let error = registry
            .add_folder("shop", folder("front", "/other/front"))
            .unwrap_err();
        assert!(error.to_string().contains("--as"));
        assert!(
            registry
                .add_folder("shop", folder("front2", "/other/front"))
                .is_ok()
        );
    }

    #[test]
    fn renames_projects_with_their_folders() {
        let mut registry = registry();
        assert!(registry.rename("shop", "blog").is_err());
        assert!(registry.rename("shop", "Bad Name").is_err());
        registry.rename("shop", "store").unwrap();
        assert!(registry.find("shop").is_none());
        let store = registry.find("store").unwrap();
        assert_eq!(store.name, "store");
        assert_eq!(store.folders.len(), 1);
    }

    #[test]
    fn detaches_only_attached_folders() {
        let mut registry = registry();
        assert!(
            registry
                .remove_folder("shop", Path::new("/code/back"))
                .is_err()
        );
        let removed = registry
            .remove_folder("shop", Path::new("/code/front"))
            .unwrap();
        assert_eq!(removed.name, "front");
        assert!(registry.find("shop").unwrap().folders.is_empty());
    }
}
