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
