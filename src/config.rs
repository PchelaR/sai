use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use etcetera::BaseStrategy;
use serde::Deserialize;

use crate::files;
use crate::tools::Permissions;
use crate::vps::SSH_PORT;

const CONFIG_FILE: &str = "config.toml";
const INSTRUCTIONS_FILE: &str = "AGENTS.md";
const EXAMPLE: &str = r#"ssh_key = "~/.ssh/id_ed25519"
ip = "1.2.3.4""#;

#[derive(Deserialize)]
pub struct Config {
    ssh_key: String,
    pub ip: String,
    port: Option<u16>,
    #[serde(default)]
    pub agents: Permissions,
    /// Docker's build cache on the VPS: kept, the agent image is rebuilt in minutes after
    /// an update of sai instead of 15-30, for a few GB of disk. Unset: as `sai setup` asked
    build_cache: Option<bool>,
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
        let config: Self =
            toml::from_str(content.trim_start_matches('\u{feff}')).with_context(|| {
                format!(
                    "invalid config at {}, write Windows paths with / or in single quotes",
                    path.display()
                )
            })?;
        config
            .agents
            .validate()
            .with_context(|| format!("invalid config at {}", path.display()))?;
        Ok(config)
    }

    /// The argument the image scripts take: yes, no, or empty to keep what is on the VPS
    pub fn build_cache(&self) -> &'static str {
        match self.build_cache {
            Some(true) => "yes",
            Some(false) => "no",
            None => "",
        }
    }

    pub fn wants_build_cache(&self) -> Option<bool> {
        self.build_cache
    }

    pub fn port(&self) -> u16 {
        self.port.unwrap_or(SSH_PORT)
    }

    pub fn custom_port(&self) -> Option<u16> {
        self.port
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

pub fn dir() -> Result<PathBuf> {
    Ok(etcetera::choose_base_strategy()?.config_dir().join("sai"))
}

#[cfg(test)]
mod tests {
    use super::Config;

    fn config(extra: &str) -> Config {
        toml::from_str(&format!(
            "ssh_key = \"~/.ssh/id\"\nip = \"1.2.3.4\"\n{extra}"
        ))
        .unwrap()
    }

    #[test]
    fn the_build_cache_is_set_by_hand_or_left_to_setup() {
        assert_eq!(config("build_cache = true").build_cache(), "yes");
        assert_eq!(config("build_cache = false").build_cache(), "no");
        assert_eq!(config("").build_cache(), "");
        assert_eq!(config("").wants_build_cache(), None);
    }
}
