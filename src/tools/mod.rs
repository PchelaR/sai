use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::agent::Agent;
use crate::vps::USER;
use crate::{ignores, remote, secrets};

const INSTALL: &str = include_str!("install.sh");
const SNAPSHOTS: &str = include_str!("snapshots.sh");
const SNAPSHOT_SERVICE: &str = include_str!("sai-snapshot.service");
const SNAPSHOT_TIMER: &str = include_str!("sai-snapshot.timer");
const CLAUDE_POLICY: &str = include_str!("claude-managed-settings.json");
const CODEX_RULES: &str = include_str!("codex.rules");
const EXECUTABLE: &str = "755";
const READABLE: &str = "644";

const CLAUDE_POLICY_FILE: &str = "claude-managed-settings.json";
const CODEX_RULES_FILE: &str = "codex.rules";
const INSTRUCTIONS_FILE: &str = "instructions.md";

pub fn snapshots_path() -> String {
    file("bin/snapshots")
}

pub fn install(instructions: &str) -> Result<()> {
    let units = format!("/home/{USER}/.config/systemd/user");
    let files = [
        (snapshots_path(), EXECUTABLE, SNAPSHOTS.to_owned()),
        (file("snapshot-excludes"), READABLE, snapshot_excludes()),
        (
            format!("{units}/sai-snapshot.service"),
            READABLE,
            SNAPSHOT_SERVICE.to_owned(),
        ),
        (
            format!("{units}/sai-snapshot.timer"),
            READABLE,
            SNAPSHOT_TIMER.to_owned(),
        ),
        (file(CLAUDE_POLICY_FILE), READABLE, CLAUDE_POLICY.to_owned()),
        (file(CODEX_RULES_FILE), READABLE, CODEX_RULES.to_owned()),
        (file(INSTRUCTIONS_FILE), READABLE, instructions.to_owned()),
    ];
    let args: Vec<String> = files
        .into_iter()
        .flat_map(|(path, mode, content)| [path, mode.to_owned(), BASE64_STANDARD.encode(content)])
        .collect();
    remote::run(
        INSTALL,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

pub fn read_only_mounts(agent: Agent) -> Vec<String> {
    let mounts: &[(&str, &str)] = match agent {
        Agent::Claude => &[
            (CLAUDE_POLICY_FILE, "/etc/claude-code/managed-settings.json"),
            (INSTRUCTIONS_FILE, "/state/CLAUDE.md"),
        ],
        Agent::Codex => &[
            (CODEX_RULES_FILE, "/state/rules/sai.rules"),
            (INSTRUCTIONS_FILE, "/state/AGENTS.md"),
        ],
    };
    mounts
        .iter()
        .map(|(name, target)| format!("{}:{target}", file(name)))
        .collect()
}

fn file(name: &str) -> String {
    format!("/home/{USER}/.sai/{name}")
}

fn snapshot_excludes() -> String {
    ignores::BUILD
        .iter()
        .chain(secrets::PATTERNS)
        .map(|pattern| format!("{pattern}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::read_only_mounts;
    use crate::agent::Agent;

    #[test]
    fn mounts_only_the_agents_own_files() {
        let claude = read_only_mounts(Agent::Claude).join(" ");
        let codex = read_only_mounts(Agent::Codex).join(" ");
        assert!(claude.contains("/etc/claude-code/managed-settings.json"));
        assert!(claude.contains("/state/CLAUDE.md"));
        assert!(!claude.contains("codex"));
        assert!(codex.contains("/state/rules/sai.rules"));
        assert!(codex.contains("/state/AGENTS.md"));
        assert!(!codex.contains("claude"));
    }
}
