pub mod change;
pub mod list;
pub mod remove;
pub mod stop;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};

use crate::vps::USER;

pub struct Project {
    pub name: String,
    pub local_path: PathBuf,
}

impl Project {
    pub fn open(dir_path: &str) -> Result<Self> {
        let local_path = fs::canonicalize(dir_path)
            .with_context(|| format!("directory not found: {dir_path}"))?;
        ensure!(
            local_path.is_dir(),
            "not a directory: {}",
            local_path.display()
        );
        let name = local_path
            .file_name()
            .map(|name| slug(&name.to_string_lossy()))
            .unwrap_or_default();
        ensure!(
            !name.is_empty(),
            "cannot derive a project name from {}",
            local_path.display()
        );
        Ok(Self { name, local_path })
    }

    pub fn id(&self) -> String {
        id(&self.name)
    }

    pub fn remote_path(&self) -> String {
        remote_path(&self.name)
    }
}

pub fn id(name: &str) -> String {
    format!("sai-{name}")
}

pub fn remote_path(name: &str) -> String {
    format!("/home/{USER}/projects/{name}")
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && slug(name) == name,
        "invalid project name: {name}"
    );
    Ok(())
}

fn slug(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::{id, remote_path, slug, validate_name};

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
    }

    #[test]
    fn derives_remote_names() {
        assert_eq!(id("app"), "sai-app");
        assert_eq!(remote_path("app"), "/home/sai/projects/app");
    }
}
