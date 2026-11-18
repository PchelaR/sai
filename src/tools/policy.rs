use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use serde_json::json;

const DELETE: &str = "Deleting files is not allowed";
const COMMIT: &str = "Commits are made by the user";
const PUSH: &str = "Pushes are made by the user";
const HISTORY: &str =
    "The agent can read git history but not change branches, history or files through git";
const USER_DENY: &str = "Forbidden in the sai config";
const BELL: &str = include_str!("bell.sh");

const BUILT_IN: &[(&str, &str)] = &[
    ("rm", DELETE),
    ("rmdir", DELETE),
    ("unlink", DELETE),
    ("shred", DELETE),
    ("git clean", DELETE),
    ("git commit", COMMIT),
    ("git push", PUSH),
    ("git checkout", HISTORY),
    ("git switch", HISTORY),
    ("git restore", HISTORY),
    ("git reset", HISTORY),
    ("git stash", HISTORY),
    ("git merge", HISTORY),
    ("git rebase", HISTORY),
    ("git cherry-pick", HISTORY),
    ("git revert", HISTORY),
    ("git pull", HISTORY),
    ("git am", HISTORY),
    ("git worktree", HISTORY),
];

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    restrictions: bool,
    allow: Vec<String>,
    deny: Vec<String>,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            restrictions: true,
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }
}

struct Rule {
    words: Vec<String>,
    reason: &'static str,
}

impl Permissions {
    pub fn validate(&self) -> Result<()> {
        let allowed = commands(&self.allow)?;
        let denied = commands(&self.deny)?;
        for allow in &allowed {
            if let Some(deny) = denied.iter().find(|deny| starts_with(allow, deny)) {
                bail!(
                    "[agents] allows {:?} but denies {:?}, remove one of them",
                    allow.join(" "),
                    deny.join(" ")
                );
            }
            if self.restrictions
                && let Some((command, _)) = BUILT_IN.iter().find(|(command, _)| {
                    let built_in = words(command);
                    starts_with(allow, &built_in) && allow.len() > built_in.len()
                })
            {
                bail!(
                    "[agents] allows {:?}, but sai forbids every {command:?} command, allow {command:?} to lift that ban",
                    allow.join(" ")
                );
            }
        }
        Ok(())
    }

    pub fn lifted(&self) -> Vec<&'static str> {
        let allowed = commands(&self.allow).unwrap_or_default();
        BUILT_IN
            .iter()
            .map(|(command, _)| *command)
            .filter(|command| !self.restrictions || covered(&words(command), &allowed))
            .collect()
    }

    pub fn claude(&self) -> Result<String> {
        let pattern = |words: &[String]| format!("Bash({}:*)", words.join(" "));
        let deny: Vec<String> = self
            .rules()?
            .iter()
            .map(|rule| pattern(&rule.words))
            .collect();
        let allow: Vec<String> = commands(&self.allow)?
            .iter()
            .map(|words| pattern(words))
            .collect();
        let bell = json!([{ "hooks": [{ "type": "command", "command": BELL, "timeout": 5 }] }]);
        let asking = json!([{
            "matcher": "permission_prompt",
            "hooks": [{ "type": "command", "command": BELL, "timeout": 5 }]
        }]);
        Ok(serde_json::to_string_pretty(&json!({
            "permissions": { "deny": deny, "allow": allow },
            "hooks": { "Stop": bell, "Notification": asking }
        }))? + "\n")
    }

    pub fn codex(&self) -> Result<String> {
        let mut text = String::new();
        for rule in self.rules()? {
            text.push_str(&format!(
                "prefix_rule(pattern = {}, decision = \"forbidden\", justification = {})\n",
                serde_json::to_string(&rule.words)?,
                serde_json::to_string(rule.reason)?
            ));
        }
        for words in commands(&self.allow)? {
            text.push_str(&format!(
                "prefix_rule(pattern = {}, decision = \"allow\")\n",
                serde_json::to_string(&words)?
            ));
        }
        Ok(text)
    }

    fn rules(&self) -> Result<Vec<Rule>> {
        let allowed = commands(&self.allow)?;
        let built_in = BUILT_IN
            .iter()
            .filter(|_| self.restrictions)
            .map(|(command, reason)| Rule {
                words: words(command),
                reason,
            })
            .filter(|rule| !covered(&rule.words, &allowed));
        let custom = commands(&self.deny)?.into_iter().map(|words| Rule {
            words,
            reason: USER_DENY,
        });
        Ok(built_in.chain(custom).collect())
    }
}

fn commands(entries: &[String]) -> Result<Vec<Vec<String>>> {
    entries
        .iter()
        .map(|entry| {
            let words = words(entry);
            ensure!(!words.is_empty(), "[agents] has an empty command");
            if let Some(word) = words.iter().find(|word| !word.chars().all(allowed_char)) {
                bail!(
                    "[agents] command {entry:?} has unsupported characters in {word:?}, use plain words like \"npm test\""
                );
            }
            Ok(words)
        })
        .collect()
}

fn allowed_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || "._/=+@,%-".contains(character)
}

fn words(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_owned).collect()
}

fn starts_with(command: &[String], prefix: &[String]) -> bool {
    command.len() >= prefix.len() && command[..prefix.len()] == *prefix
}

fn covered(rule: &[String], allowed: &[Vec<String>]) -> bool {
    allowed.iter().any(|allow| starts_with(rule, allow))
}

#[cfg(test)]
mod tests {
    use super::Permissions;

    fn parse(text: &str) -> Permissions {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn keeps_the_built_in_rules_by_default() {
        let permissions = Permissions::default();
        permissions.validate().unwrap();
        let claude = permissions.claude().unwrap();
        assert!(claude.contains("\"Bash(rm:*)\""));
        assert!(claude.contains("\"Bash(git worktree:*)\""));
        assert_eq!(claude.matches("Bash(").count(), 19);
        let codex = permissions.codex().unwrap();
        assert!(codex.contains(
            "prefix_rule(pattern = [\"git\",\"commit\"], decision = \"forbidden\", justification = \"Commits are made by the user\")"
        ));
        assert_eq!(codex.lines().count(), 19);
        assert!(permissions.lifted().is_empty());
    }

    #[test]
    fn rings_the_bell_when_claude_finishes_or_asks() {
        let claude: serde_json::Value =
            serde_json::from_str(&Permissions::default().claude().unwrap()).unwrap();
        let hooks = &claude["hooks"];
        assert_eq!(hooks["Notification"][0]["matcher"], "permission_prompt");
        for event in ["Stop", "Notification"] {
            let command = hooks[event][0]["hooks"][0]["command"].as_str().unwrap();
            assert!(command.contains("printf '\\a'"));
            assert!(command.contains("focused"));
        }
    }

    #[test]
    fn allowing_lifts_matching_built_in_rules() {
        let permissions = parse(
            r#"allow = ["git", "npm test"]
deny = ["curl"]"#,
        );
        permissions.validate().unwrap();
        let claude = permissions.claude().unwrap();
        assert!(!claude.contains("Bash(git "));
        assert!(claude.contains("\"Bash(rm:*)\""));
        assert!(claude.contains("\"Bash(curl:*)\""));
        assert!(claude.contains("\"Bash(npm test:*)\""));
        let codex = permissions.codex().unwrap();
        assert!(codex.contains("prefix_rule(pattern = [\"npm\",\"test\"], decision = \"allow\")"));
        assert!(codex.contains("[\"curl\"], decision = \"forbidden\""));
        assert_eq!(permissions.lifted().len(), 15);
    }

    #[test]
    fn turning_restrictions_off_keeps_only_the_users_rules() {
        let permissions = parse("restrictions = false\ndeny = [\"curl\"]");
        permissions.validate().unwrap();
        assert_eq!(permissions.claude().unwrap().matches("Bash(").count(), 1);
        assert_eq!(permissions.lifted().len(), 19);
        permissions.validate().unwrap();
        let narrow = parse("restrictions = false\nallow = [\"rm -rf build\"]");
        narrow.validate().unwrap();
    }

    #[test]
    fn rejects_unclear_rules() {
        assert!(parse(r#"allow = ["rm -rf build"]"#).validate().is_err());
        assert!(
            parse(
                r#"allow = ["git push"]
deny = ["git"]"#
            )
            .validate()
            .is_err()
        );
        assert!(parse(r#"allow = ["npm \"x\""]"#).validate().is_err());
        assert!(parse(r#"deny = ["  "]"#).validate().is_err());
        assert!(toml::from_str::<Permissions>("alow = []").is_err());
    }
}
