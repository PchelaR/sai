use anyhow::{Result, bail};
use base64::{Engine, prelude::BASE64_STANDARD};
use clap::ValueEnum;

use crate::fingerprint;

pub const SESSION: &str = "agent";
pub const COMMAND: &str = "agent";

#[derive(Clone, Copy, Default, ValueEnum)]
pub enum Agent {
    #[default]
    Claude,
    Codex,
}

pub struct Image {
    repository: &'static str,
    dockerfile: &'static str,
    pub policy: Policy,
}

pub struct Policy {
    pub name: &'static str,
    pub content: &'static str,
    pub container_path: &'static str,
}

impl Agent {
    pub fn image(self) -> Result<Image> {
        match self {
            Agent::Claude => Ok(Image {
                repository: "sai-agent-claude",
                dockerfile: include_str!("agent/claude.Dockerfile"),
                policy: Policy {
                    name: "claude-managed-settings.json",
                    content: include_str!("agent/claude-managed-settings.json"),
                    container_path: "/etc/claude-code/managed-settings.json",
                },
            }),
            Agent::Codex => bail!("codex agent is not supported yet"),
        }
    }
}

impl Image {
    pub fn reference(&self) -> String {
        format!("{}:{}", self.repository, fingerprint::of(self.dockerfile))
    }

    pub fn encoded_dockerfile(&self) -> String {
        BASE64_STANDARD.encode(self.dockerfile)
    }
}
