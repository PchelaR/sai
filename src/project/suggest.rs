use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use super::repos;

const BUILD_OUTPUT: &[&str] = &[
    "build", "dist", "out", "coverage", "tmp", ".idea", ".vs", ".eggs",
];

pub struct Suggestions {
    paths: BTreeSet<String>,
    repositories: Vec<String>,
}

impl Suggestions {
    pub fn find(root: &Path) -> Self {
        let mut paths = BTreeSet::new();
        let mut repositories = Vec::new();
        if let Some(ignored) = ignored_by_git(root) {
            paths.extend(ignored);
            repositories.push(String::new());
        }
        for relative in repos::find(root) {
            let label = repos::label(&relative);
            if label == "." || repositories.iter().any(|repository| repository.is_empty()) {
                continue;
            }
            if let Some(ignored) = ignored_by_git(&root.join(&relative)) {
                paths.extend(ignored.into_iter().map(|path| format!("{label}/{path}")));
                repositories.push(label);
            }
        }
        Self {
            paths,
            repositories,
        }
    }

    pub fn reason(&self, path: &str, name: &str, is_dir: bool) -> Option<&'static str> {
        if self.paths.contains(path) {
            return Some("in .gitignore");
        }
        let in_git = self
            .repositories
            .iter()
            .any(|repository| super::content::is_within(path, repository));
        (!in_git && is_dir && BUILD_OUTPUT.contains(&name)).then_some("build output")
    }
}

fn ignored_by_git(dir: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                String::from_utf8_lossy(path)
                    .trim_end_matches('/')
                    .to_owned()
            })
            .filter(|path| !path.starts_with("../"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::Suggestions;

    #[test]
    fn suggests_what_git_ignores_and_build_output() {
        let root = std::env::temp_dir().join(format!("sai-suggest-{}", std::process::id()));
        fs::create_dir_all(root.join("app/build")).unwrap();
        fs::create_dir_all(root.join("app/src")).unwrap();
        fs::create_dir_all(root.join("tools/build")).unwrap();
        fs::write(root.join("app/.gitignore"), "build/\n*.log\n").unwrap();
        fs::write(root.join("app/build/x"), "x").unwrap();
        fs::write(root.join("app/debug.log"), "x").unwrap();
        fs::write(root.join("app/src/main.py"), "x").unwrap();
        fs::write(root.join("tools/build/y"), "y").unwrap();
        let git = Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.join("app"))
            .status();
        if !git.is_ok_and(|status| status.success()) {
            fs::remove_dir_all(root).unwrap();
            return;
        }
        let suggestions = Suggestions::find(&root);
        assert_eq!(
            suggestions.reason("app/build", "build", true),
            Some("in .gitignore")
        );
        assert_eq!(
            suggestions.reason("app/debug.log", "debug.log", false),
            Some("in .gitignore")
        );
        assert_eq!(suggestions.reason("app/src", "src", true), None);
        assert_eq!(
            suggestions.reason("tools/build", "build", true),
            Some("build output")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
