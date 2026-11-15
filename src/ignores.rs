use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::files;

pub const FILE: &str = ".saiignore";
const MAX_DEPTH: usize = 8;

pub const BUILD: &[&str] = &[
    ".venv/",
    "venv/",
    "__pycache__/",
    ".tox/",
    ".nox/",
    ".mypy_cache/",
    ".pytest_cache/",
    ".ruff_cache/",
    ".hypothesis/",
    ".ipynb_checkpoints/",
    "*.egg-info/",
    "htmlcov/",
    "node_modules/",
    "bower_components/",
    ".next/",
    ".nuxt/",
    ".svelte-kit/",
    ".angular/",
    ".turbo/",
    ".parcel-cache/",
    ".expo/",
    ".nyc_output/",
    "target/",
    ".gradle/",
    ".kotlin/",
    ".dart_tool/",
    "DerivedData/",
    "_build/",
    ".stack-work/",
    "dist-newstyle/",
    ".zig-cache/",
    "zig-cache/",
    "zig-out/",
    "cmake-build-*/",
    ".terraform/",
    ".cache/",
    ".DS_Store",
    "Thumbs.db",
];

pub fn dirs(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![(PathBuf::new(), 0)];
    while let Some((relative, depth)) = pending.pop() {
        if depth < max_depth
            && let Ok(entries) = fs::read_dir(root.join(&relative))
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) && !skipped_dir(&name) {
                    pending.push((relative.join(name), depth + 1));
                }
            }
        }
        found.push(relative);
    }
    found.sort_by_key(|relative| (relative.components().count(), relative.clone()));
    found
}

pub fn custom(root: &Path) -> Result<Vec<String>> {
    let mut patterns = Vec::new();
    for relative in dirs(root, MAX_DEPTH) {
        let text = files::read_or_empty(&root.join(&relative).join(FILE))?;
        let base = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        patterns.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(|line| scoped(&base, line)),
        );
    }
    Ok(patterns)
}

pub struct Rules {
    patterns: Vec<Rule>,
}

struct Rule {
    negation: bool,
    dir_only: bool,
    anchored: bool,
    segments: Vec<String>,
}

impl Rules {
    pub fn load(root: &Path) -> Self {
        Self::parse(&custom(root).unwrap_or_default())
    }

    fn parse(patterns: &[String]) -> Self {
        let patterns = patterns
            .iter()
            .filter_map(|line| {
                let (negation, pattern) = match line.strip_prefix('!') {
                    Some(pattern) => (true, pattern),
                    None => (false, line.as_str()),
                };
                let dir_only = pattern.ends_with('/');
                let pattern = pattern.trim_end_matches('/');
                let anchored = pattern.contains('/');
                let segments: Vec<String> = pattern
                    .trim_start_matches('/')
                    .split('/')
                    .map(str::to_owned)
                    .collect();
                (!pattern.is_empty()).then_some(Rule {
                    negation,
                    dir_only,
                    anchored,
                    segments,
                })
            })
            .collect();
        Self { patterns }
    }

    pub fn ignored(&self, path: &str, is_dir: bool) -> bool {
        let parts: Vec<&str> = path.split('/').collect();
        let mut ignored = false;
        for rule in &self.patterns {
            if rule.dir_only && !is_dir {
                continue;
            }
            let matched = if rule.anchored {
                segments_match(&rule.segments, &parts)
            } else {
                parts
                    .last()
                    .is_some_and(|name| glob(&rule.segments[0], name))
            };
            if matched {
                ignored = !rule.negation;
            }
        }
        ignored
    }
}

fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((first, rest)) => path
            .split_first()
            .is_some_and(|(name, others)| glob(first, name) && segments_match(rest, others)),
    }
}

fn glob(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    glob_chars(&pattern, &name)
}

fn glob_chars(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skip| glob_chars(rest, &name[skip..])),
        Some(('?', rest)) => !name.is_empty() && glob_chars(rest, &name[1..]),
        Some(('\\', rest)) if !rest.is_empty() => {
            name.first() == Some(&rest[0]) && glob_chars(&rest[1..], &name[1..])
        }
        Some((character, rest)) => name.first() == Some(character) && glob_chars(rest, &name[1..]),
    }
}

pub fn anchored(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if "*?[]{}\\!".contains(character) {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn scoped(base: &str, line: &str) -> String {
    if base.is_empty() {
        return line.to_owned();
    }
    let (negation, pattern) = match line.strip_prefix('!') {
        Some(pattern) => ("!", pattern),
        None => ("", line),
    };
    let anchored = pattern.starts_with('/') || pattern.trim_end_matches('/').contains('/');
    let pattern = pattern.trim_start_matches('/');
    if anchored {
        format!("{negation}/{base}/{pattern}")
    } else {
        format!("{negation}/{base}/**/{pattern}")
    }
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || BUILD
            .iter()
            .filter_map(|pattern| pattern.strip_suffix('/'))
            .any(|pattern| matches(pattern, name))
}

pub fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
        None => pattern == name,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{BUILD, FILE, anchored, custom, matches, scoped};

    #[test]
    fn applies_ignore_rules_like_mutagen() {
        let rules = super::Rules::parse(&[
            "*.log".to_owned(),
            "/data/raw/".to_owned(),
            "/app/**/weights/".to_owned(),
            "!keep.log".to_owned(),
            "/my \\[old\\].bin".to_owned(),
        ]);
        assert!(rules.ignored("debug.log", false));
        assert!(rules.ignored("src/deep/debug.log", false));
        assert!(!rules.ignored("src/keep.log", false));
        assert!(rules.ignored("data/raw", true));
        assert!(!rules.ignored("data/raw", false));
        assert!(!rules.ignored("other/data/raw", true));
        assert!(rules.ignored("app/weights", true));
        assert!(rules.ignored("app/a/b/weights", true));
        assert!(!rules.ignored("weights", true));
        assert!(rules.ignored("my [old].bin", false));
        assert!(!rules.ignored("src/main.rs", false));
    }

    #[test]
    fn anchors_skipped_paths() {
        assert_eq!(anchored("data/raw"), "/data/raw");
        assert_eq!(anchored("my [old] *.bin"), "/my \\[old\\] \\*.bin");
    }

    #[test]
    fn scopes_nested_patterns() {
        assert_eq!(scoped("", "dist/"), "dist/");
        assert_eq!(scoped("app", "dist/"), "/app/**/dist/");
        assert_eq!(scoped("app/web", "/out"), "/app/web/out");
        assert_eq!(scoped("app", "data/raw/"), "/app/data/raw/");
        assert_eq!(scoped("app", "!keep.log"), "!/app/**/keep.log");
    }

    #[test]
    fn reads_ignore_files_in_subfolders() {
        let root = std::env::temp_dir().join(format!("sai-ignores-{}", std::process::id()));
        fs::create_dir_all(root.join("tionix/data")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join(FILE), "*.log\n").unwrap();
        fs::write(
            root.join("tionix").join(FILE),
            "# comment\n/data/\nweights/\n",
        )
        .unwrap();
        fs::write(root.join("node_modules/pkg").join(FILE), "x\n").unwrap();
        assert_eq!(
            custom(&root).unwrap(),
            ["*.log", "/tionix/data/", "/tionix/**/weights/"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matches_simple_globs() {
        assert!(matches("*.egg-info", "sai.egg-info"));
        assert!(matches("cmake-build-*", "cmake-build-debug"));
        assert!(!matches("*.egg-info", "egg-info"));
        assert!(matches("venv", "venv"));
        assert!(!matches("venv", "venvs"));
    }

    #[test]
    fn patterns_are_unique_and_have_at_most_one_star() {
        for (index, pattern) in BUILD.iter().enumerate() {
            assert!(
                !BUILD[..index].contains(pattern),
                "{pattern} is listed twice"
            );
            assert!(pattern.matches('*').count() <= 1, "{pattern}");
        }
    }
}
