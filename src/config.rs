use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use etcetera::BaseStrategy;
use serde::Deserialize;

use crate::files;

const CONFIG_FILE: &str = "config.toml";
const INSTRUCTIONS_FILE: &str = "AGENTS.md";
const EXAMPLE: &str = r#"ssh_key = "~/.ssh/id_ed25519"
ip = "1.2.3.4""#;

#[derive(Deserialize)]
pub struct Config {
    ssh_key: String,
    pub ip: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = path()?;
        let content = fs::read_to_string(&path).with_context(|| {
            format!(
                "config not found at {}, create it with:\n\n{EXAMPLE}\n",
                path.display()
            )
        })?;
        toml::from_str(&content).with_context(|| format!("invalid config at {}", path.display()))
    }

    pub fn private_key(&self) -> Result<PathBuf> {
        match self.ssh_key.strip_prefix("~/") {
            Some(relative) => Ok(etcetera::home_dir()?.join(relative)),
            None => Ok(PathBuf::from(&self.ssh_key)),
        }
    }

    pub fn public_key(&self) -> Result<String> {
        let mut path = self.private_key()?.into_os_string();
        path.push(".pub");
        let path = PathBuf::from(path);
        let key = fs::read_to_string(&path)
            .with_context(|| format!("public key not found at {}", path.display()))?;
        let key = key.trim();
        ensure!(!key.is_empty(), "public key at {} is empty", path.display());
        Ok(key.to_owned())
    }
}

pub fn instructions() -> Result<String> {
    files::read_or_empty(&dir()?.join(INSTRUCTIONS_FILE))
}

fn path() -> Result<PathBuf> {
    Ok(dir()?.join(CONFIG_FILE))
}

fn dir() -> Result<PathBuf> {
    Ok(etcetera::choose_base_strategy()?.config_dir().join("sai"))
}
