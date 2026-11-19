mod policy;

use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::agent::Agent;
use crate::vps::USER;
use crate::{ignores, secrets};

const INSTALL: &str = include_str!("install.sh");
const SNAPSHOTS: &str = include_str!("snapshots.sh");
const SNAPSHOT_SERVICE: &str = include_str!("sai-snapshot.service");
const SNAPSHOT_TIMER: &str = include_str!("sai-snapshot.timer");
const TMUX: &str = include_str!("tmux.conf");
const TURN_REPORT: &str = include_str!("turn_report.sh");
const TURN_REPORT_FILE: &str = "turn-report";
const EXCLUDES_FILE: &str = "snapshot-excludes";
const EXCLUDES_TARGET: &str = "/opt/sai/excludes";
const CODEX_REQUIREMENTS_FILE: &str = "codex-requirements.toml";
// Codex runs hooks from requirements.toml as managed: trusted and not switchable by the agent
const CODEX_REQUIREMENTS: &str = r#"[[hooks.UserPromptSubmit]]
[[hooks.UserPromptSubmit.hooks]]
type = "command"
command = "/opt/sai/turn-report start"
timeout = 60

[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "/opt/sai/turn-report report"
timeout = 60
"#;
const WATCHED: &[&str] = &[
    ".saiignore",
    ".gitignore",
    ".gitattributes",
    ".gitmodules",
    ".envrc",
    ".tool-versions",
    "mise.toml",
    ".mise.toml",
    "package.json",
    ".npmrc",
    "Makefile",
    "makefile",
    "GNUmakefile",
    "justfile",
    "Justfile",
    "Taskfile.yml",
    "build.rs",
    "setup.py",
    "pyproject.toml",
    ".pre-commit-config.yaml",
    "lefthook.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "*.code-workspace",
    ".husky/",
    ".githooks/",
    ".git-hooks/",
    ".vscode/",
    ".idea/",
    ".devcontainer/",
    ".github/workflows/",
    ".cargo/",
    "Cargo.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".yarnrc",
    ".yarnrc.yml",
    ".pnpmfile.cjs",
    "bunfig.toml",
    "gradlew",
    "gradlew.bat",
    "*.gradle",
    "*.gradle.kts",
    "gradle/wrapper/",
    "mvnw",
    "mvnw.cmd",
    "pom.xml",
    ".mvn/",
    ".gitlab-ci.yml",
    ".circleci/",
    "Jenkinsfile",
    // Settings and instructions of coding agents run on this computer: hooks, MCP servers
    ".claude/",
    ".mcp.json",
    ".codex/",
    ".cursor/",
    ".zed/",
    "CLAUDE.md",
    "AGENTS.md",
];
const EXECUTABLE: &str = "755";
const READABLE: &str = "644";

const CLAUDE_POLICY_FILE: &str = "claude-managed-settings.json";
const CODEX_RULES_FILE: &str = "codex.rules";
const TMUX_FILE: &str = "tmux.conf";
pub const TMUX_TARGET: &str = "/home/agent/.tmux.conf";
const INSTRUCTIONS_FILE: &str = "instructions.md";

pub fn snapshots_path() -> String {
    file("bin/snapshots")
}

pub use policy::Permissions;

pub fn install_step(
    instructions: &str,
    permissions: &Permissions,
) -> Result<(&'static str, Vec<String>)> {
    let units = format!("/home/{USER}/.config/systemd/user");
    let files = [
        (snapshots_path(), EXECUTABLE, SNAPSHOTS.to_owned()),
        (file(EXCLUDES_FILE), READABLE, snapshot_excludes()),
        (file(TURN_REPORT_FILE), EXECUTABLE, TURN_REPORT.to_owned()),
        (
            file(CODEX_REQUIREMENTS_FILE),
            READABLE,
            CODEX_REQUIREMENTS.to_owned(),
        ),
        (file("watched-files"), READABLE, watched_files()),
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
        (file(CLAUDE_POLICY_FILE), READABLE, permissions.claude()?),
        (file(CODEX_RULES_FILE), READABLE, permissions.codex()?),
        (file(TMUX_FILE), READABLE, TMUX.to_owned()),
        (file(INSTRUCTIONS_FILE), READABLE, instructions.to_owned()),
    ];
    let args: Vec<String> = files
        .into_iter()
        .flat_map(|(path, mode, content)| [path, mode.to_owned(), BASE64_STANDARD.encode(content)])
        .collect();
    Ok((INSTALL, args))
}

pub fn read_only_mounts(agent: Agent, project: &str) -> Vec<String> {
    let (policy, policy_target, instructions_target) = match agent {
        Agent::Claude => (
            CLAUDE_POLICY_FILE,
            "/etc/claude-code/managed-settings.json",
            "/state/CLAUDE.md",
        ),
        Agent::Codex => (
            CODEX_RULES_FILE,
            "/state/rules/sai.rules",
            "/state/AGENTS.md",
        ),
    };
    let mut mounts = vec![
        format!("{}:{policy_target}", file(policy)),
        format!("{}:{instructions_target}", instructions_path(project)),
        format!("{}:{TMUX_TARGET}", file(TMUX_FILE)),
        // What the agent changed in each answer, shown to the user
        format!("{}:{}", file(TURN_REPORT_FILE), policy::TURN_REPORT_TARGET),
        format!("{}:{EXCLUDES_TARGET}", file(EXCLUDES_FILE)),
    ];
    if agent == Agent::Codex {
        mounts.push(format!(
            "{}:/etc/codex/requirements.toml",
            file(CODEX_REQUIREMENTS_FILE)
        ));
    }
    mounts
}

/// The agent's instructions for one project: the shared ones from the sai config and how
/// to reach the project's services. Next to the agent's state, outside of what it can write
pub fn instructions_path(project: &str) -> String {
    format!("/home/{USER}/agents/{project}/instructions.md")
}

fn file(name: &str) -> String {
    format!("/home/{USER}/.sai/{name}")
}

fn watched_files() -> String {
    WATCHED
        .iter()
        .map(|pattern| format!("{pattern}\n"))
        .collect()
}

fn snapshot_excludes() -> String {
    ignores::BUILD
        .iter()
        .chain(secrets::PATTERNS)
        .chain(&[".git/"])
        .map(|pattern| format!("{pattern}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::read_only_mounts;
    use crate::agent::Agent;

    #[test]
    fn mounts_only_the_agents_own_files() {
        let claude = read_only_mounts(Agent::Claude, "shop").join(" ");
        let codex = read_only_mounts(Agent::Codex, "shop").join(" ");
        assert!(claude.contains("/etc/claude-code/managed-settings.json"));
        assert!(claude.contains("/state/CLAUDE.md"));
        assert!(!claude.contains("codex"));
        assert!(codex.contains("/state/rules/sai.rules"));
        assert!(codex.contains("/state/AGENTS.md"));
        assert!(!codex.contains("claude"));
        for mounts in [&claude, &codex] {
            assert!(mounts.contains("/home/agent/.tmux.conf"));
            assert!(mounts.contains("/home/sai/agents/shop/instructions.md:"));
        }
    }
}
