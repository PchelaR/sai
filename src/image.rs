use base64::{Engine, prelude::BASE64_STANDARD};

use crate::agent::Agent;
use crate::fingerprint;

const BASE_REPOSITORY: &str = "sai-base";
const BASE_DOCKERFILE: &str = include_str!("image/base.Dockerfile");

pub fn base() -> String {
    format!("{BASE_REPOSITORY}:{}", fingerprint::of(BASE_DOCKERFILE))
}

pub fn agent(agent: Agent) -> String {
    let dockerfiles = format!("{BASE_DOCKERFILE}{}", agent.dockerfile());
    format!("sai-{agent}:{}", fingerprint::of(&dockerfiles))
}

pub fn encoded_base() -> String {
    BASE64_STANDARD.encode(BASE_DOCKERFILE)
}

pub fn encoded_agent(agent: Agent) -> String {
    BASE64_STANDARD.encode(agent.dockerfile())
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::{agent, base};
    use crate::agent::Agent;

    #[test]
    fn agent_images_are_named_after_agents() {
        assert!(agent(Agent::Claude).starts_with("sai-claude:"));
        assert!(agent(Agent::Codex).starts_with("sai-codex:"));
        assert!(agent(Agent::Kimi).starts_with("sai-kimi:"));
        assert!(base().starts_with("sai-base:"));
    }

    #[test]
    fn agent_images_differ() {
        let mut references: Vec<String> =
            Agent::value_variants().iter().map(|a| agent(*a)).collect();
        let count = references.len();
        references.sort();
        references.dedup();
        assert_eq!(references.len(), count);
    }
}
