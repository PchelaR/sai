use std::fmt;

use clap::ValueEnum;

const CLAUDE: &str = "claude --permission-mode acceptEdits";
const CODEX: &str = "codex --sandbox danger-full-access --ask-for-approval on-request";
const CODEX_RESUMED: &str = "-c sandbox_mode=danger-full-access -c approval_policy=on-request";

pub enum Start {
    Ask,
    Last,
    New,
    Open(String),
}

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

    pub fn command(self, start: &Start) -> String {
        match (self, start) {
            (Agent::Claude, Start::Last) => format!(
                "if ls /state/projects/*/*.jsonl >/dev/null 2>&1; then {CLAUDE} --continue; else {CLAUDE}; fi"
            ),
            (Agent::Claude, Start::Open(chat)) => format!("{CLAUDE} --resume {chat}"),
            (Agent::Claude, _) => CLAUDE.to_owned(),
            (Agent::Codex, Start::Last) => format!(
                "if [ -n \"$(ls -A /state/sessions 2>/dev/null)\" ]; then codex resume --last {CODEX_RESUMED}; else {CODEX}; fi"
            ),
            (Agent::Codex, Start::Open(chat)) => format!("codex resume {chat} {CODEX_RESUMED}"),
            (Agent::Codex, _) => CODEX.to_owned(),
        }
    }

    pub fn login_file(self) -> &'static str {
        match self {
            Agent::Claude => ".credentials.json",
            Agent::Codex => "auth.json",
        }
    }

    pub fn sign_in_pages(self) -> &'static [&'static str] {
        match self {
            Agent::Claude => &[
                "https://claude.com/",
                "https://claude.ai/",
                "https://platform.claude.com/",
                "https://console.anthropic.com/",
            ],
            Agent::Codex => &["https://auth.openai.com/", "https://chatgpt.com/"],
        }
    }

    pub fn login_files(self) -> &'static str {
        match self {
            Agent::Claude => ".credentials.json .claude.json settings.json",
            Agent::Codex => "auth.json config.toml",
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
