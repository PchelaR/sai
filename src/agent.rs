use std::fmt;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    #[default]
    Claude,
    Codex,
}

impl Agent {
    pub fn parse(name: &str) -> Option<Agent> {
        Agent::value_variants()
            .iter()
            .copied()
            .find(|agent| agent.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Agent::Claude => "claude --permission-mode acceptEdits",
            Agent::Codex => "codex --sandbox danger-full-access --ask-for-approval on-request",
        }
    }

    pub fn dockerfile(self) -> &'static str {
        match self {
            Agent::Claude => include_str!("agent/claude.Dockerfile"),
            Agent::Codex => include_str!("agent/codex.Dockerfile"),
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::Agent;

    #[test]
    fn parses_its_own_names() {
        for agent in Agent::value_variants() {
            assert_eq!(Agent::parse(agent.name()), Some(*agent));
        }
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(Agent::parse("").is_none());
        assert!(Agent::parse("gpt").is_none());
    }

    #[test]
    fn defaults_to_claude() {
        assert_eq!(Agent::default(), Agent::Claude);
    }
}
