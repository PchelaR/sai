use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::suggest::Suggestions;
use crate::ignores::{self, Rules};
use crate::secrets;

const VCS: &[&str] = &[".git", ".hg", ".svn"];

pub struct Entry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub ignored: bool,
    pub suggested: Option<&'static str>,
    pub children: Option<Vec<Entry>>,
}

pub struct Selection {
    pub skip: Vec<String>,
    pub uploaded: u64,
    pub total: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Check {
    On,
    Partial,
    Off,
}

pub fn scan(root: &Path) -> Entry {
    let mut entry = Entry {
        name: String::new(),
        path: String::new(),
        size: 0,
        ignored: false,
        suggested: None,
        children: Some(Vec::new()),
    };
    let context = Context {
        rules: Rules::load(root),
        suggestions: Suggestions::find(root),
    };
    fill(root, &mut entry, &context);
    entry
}

struct Context {
    rules: Rules,
    suggestions: Suggestions,
}

fn fill(dir: &Path, entry: &mut Entry, context: &Context) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    let mut children = Vec::new();
    for item in listing.flatten() {
        let Ok(kind) = item.file_type() else {
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        if hidden(&name, kind.is_dir()) {
            continue;
        }
        let path = if entry.path.is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", entry.path)
        };
        let ignored = context.rules.ignored(&path, kind.is_dir());
        let suggested = context.suggestions.reason(&path, &name, kind.is_dir());
        let mut child = Entry {
            name,
            path,
            size: 0,
            ignored,
            suggested,
            children: None,
        };
        if kind.is_dir() {
            child.children = Some(Vec::new());
            if !ignored {
                fill(&item.path(), &mut child, context);
            }
        } else if kind.is_file() {
            child.size = item.metadata().map_or(0, |metadata| metadata.len());
        }
        children.push(child);
    }
    children.sort_by(|left, right| {
        right
            .children
            .is_some()
            .cmp(&left.children.is_some())
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entry.size = children
        .iter()
        .filter(|child| !child.ignored)
        .map(|child| child.size)
        .sum();
    entry.children = Some(children);
}

pub fn hidden(name: &str, is_dir: bool) -> bool {
    VCS.contains(&name)
        || ignores::BUILD
            .iter()
            .chain(secrets::PATTERNS)
            .any(|pattern| match pattern.strip_suffix('/') {
                Some(pattern) => is_dir && ignores::matches(pattern, name),
                None => ignores::matches(pattern, name),
            })
}

impl Entry {
    pub fn find(&self, path: &str) -> Option<&Entry> {
        if path.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for part in path.split('/') {
            current = current
                .children
                .as_ref()?
                .iter()
                .find(|child| child.name == part)?;
        }
        Some(current)
    }

    pub fn suggested_skip(&self) -> BTreeSet<String> {
        let mut skip = BTreeSet::new();
        self.collect_suggested(&mut skip);
        collapse(self, &mut skip);
        skip
    }

    fn collect_suggested(&self, skip: &mut BTreeSet<String>) {
        for child in self.selectable() {
            if child.suggested.is_some() {
                skip.insert(child.path.clone());
            } else {
                child.collect_suggested(skip);
            }
        }
    }

    pub fn selectable(&self) -> impl Iterator<Item = &Entry> {
        self.children
            .iter()
            .flatten()
            .filter(|child| !child.ignored)
    }

    pub fn selection(&self, skip: &BTreeSet<String>) -> Selection {
        Selection {
            skip: skip.iter().cloned().collect(),
            uploaded: self.uploaded(skip),
            total: self.size,
        }
    }

    pub fn nothing_selected(&self, skip: &BTreeSet<String>) -> bool {
        let mut children = self.selectable().peekable();
        children.peek().is_some() && children.all(|child| check(&child.path, skip) == Check::Off)
    }

    pub fn uploaded(&self, skip: &BTreeSet<String>) -> u64 {
        if self.ignored {
            return 0;
        }
        match check(&self.path, skip) {
            Check::Off => 0,
            Check::On => self.size,
            Check::Partial => self
                .children
                .iter()
                .flatten()
                .map(|child| child.uploaded(skip))
                .sum(),
        }
    }
}

pub fn check(path: &str, skip: &BTreeSet<String>) -> Check {
    if skip.iter().any(|skipped| is_within(path, skipped)) {
        Check::Off
    } else if skip
        .iter()
        .any(|skipped| skipped != path && is_within(skipped, path))
    {
        Check::Partial
    } else {
        Check::On
    }
}

pub fn toggle(root: &Entry, skip: &mut BTreeSet<String>, path: &str) {
    flip(root, skip, path);
    collapse(root, skip);
}

fn collapse(entry: &Entry, skip: &mut BTreeSet<String>) -> bool {
    if skip.contains(&entry.path) {
        return true;
    }
    let Some(children) = entry
        .children
        .as_deref()
        .filter(|children| !children.is_empty())
    else {
        return false;
    };
    let mut all = true;
    for child in children {
        all &= collapse(child, skip);
    }
    if all && !entry.path.is_empty() {
        for child in children {
            skip.remove(&child.path);
        }
        skip.insert(entry.path.clone());
    }
    all
}

fn flip(root: &Entry, skip: &mut BTreeSet<String>, path: &str) {
    match check(path, skip) {
        Check::On if !path.is_empty() => {
            skip.retain(|skipped| !is_within(skipped, path));
            skip.insert(path.to_owned());
        }
        Check::On => {
            for child in root.selectable() {
                skip.insert(child.path.clone());
            }
        }
        Check::Partial => skip.retain(|skipped| !is_within(skipped, path)),
        Check::Off => {
            if let Some(ancestor) = skip
                .iter()
                .find(|skipped| is_within(path, skipped))
                .cloned()
            {
                skip.remove(&ancestor);
                let mut current = root.find(&ancestor);
                while let Some(node) = current.filter(|node| node.path != path) {
                    let mut next = None;
                    for child in node.selectable() {
                        if is_within(path, &child.path) {
                            next = Some(child);
                        } else {
                            skip.insert(child.path.clone());
                        }
                    }
                    current = next;
                }
            }
            skip.retain(|skipped| !is_within(skipped, path));
        }
    }
}

pub fn label(entry: &Entry, skip: &BTreeSet<String>) -> String {
    let name = if entry.children.is_some() {
        format!("{}/", entry.name)
    } else {
        entry.name.clone()
    };
    let size = super::size::human(entry.size);
    if entry.ignored {
        return format!("{name}  ignored by .saiignore");
    }
    let text = match check(&entry.path, skip) {
        Check::Partial => format!(
            "{name}  {} of {size}",
            super::size::human(entry.uploaded(skip))
        ),
        _ => format!("{name}  {size}"),
    };
    match entry.suggested {
        Some(reason) => format!("{text}  ({reason})"),
        None => text,
    }
}

pub fn is_within(path: &str, ancestor: &str) -> bool {
    ancestor.is_empty()
        || path == ancestor
        || path
            .strip_prefix(ancestor)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use super::{Check, Entry, check, scan, toggle};

    fn tree(name: &str) -> (std::path::PathBuf, Entry) {
        let root = std::env::temp_dir().join(format!("sai-{name}-{}", std::process::id()));
        fs::create_dir_all(root.join("data/raw")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/x")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("data/raw/a.bin"), [0u8; 100]).unwrap();
        fs::write(root.join("data/raw/b.bin"), [0u8; 50]).unwrap();
        fs::write(root.join("data/notes.txt"), [0u8; 10]).unwrap();
        fs::write(root.join("src/main.rs"), [0u8; 5]).unwrap();
        fs::write(root.join("node_modules/x/big"), [0u8; 999]).unwrap();
        fs::write(root.join(".env"), "SECRET=1").unwrap();
        fs::write(root.join("README.md"), [0u8; 1]).unwrap();
        fs::create_dir_all(root.join("logs")).unwrap();
        fs::write(root.join("logs/app.log"), [0u8; 77]).unwrap();
        fs::write(root.join(".saiignore"), "logs/\n").unwrap();
        let entry = scan(&root);
        (root, entry)
    }

    fn names(entry: &Entry) -> Vec<&str> {
        entry
            .children
            .iter()
            .flatten()
            .map(|child| child.name.as_str())
            .collect()
    }

    #[test]
    fn hides_junk_and_secrets_and_sums_sizes() {
        let (root, entry) = tree("scan");
        assert_eq!(
            names(&entry),
            ["data", "logs", "src", ".saiignore", "README.md"]
        );
        assert!(entry.find("logs").unwrap().ignored);
        assert!(entry.suggested_skip().is_empty());
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::write(root.join("dist/app.js"), [0u8; 3]).unwrap();
        let rescanned = scan(&root);
        assert_eq!(
            rescanned.find("dist").unwrap().suggested,
            Some("build output")
        );
        assert_eq!(
            rescanned.suggested_skip().into_iter().collect::<Vec<_>>(),
            ["dist"]
        );
        assert_eq!(entry.size, 172);
        assert_eq!(entry.find("data/raw").unwrap().size, 150);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggles_like_a_torrent_client() {
        let (root, entry) = tree("toggle");
        let mut skip = BTreeSet::new();
        toggle(&entry, &mut skip, "data");
        assert_eq!(check("data/raw/a.bin", &skip), Check::Off);
        assert_eq!(check("", &skip), Check::Partial);
        assert_eq!(entry.uploaded(&skip), 12);

        toggle(&entry, &mut skip, "data/raw/a.bin");
        assert_eq!(check("data/raw/a.bin", &skip), Check::On);
        assert_eq!(check("data/raw/b.bin", &skip), Check::Off);
        assert_eq!(check("data/notes.txt", &skip), Check::Off);
        assert_eq!(check("data", &skip), Check::Partial);
        assert_eq!(entry.uploaded(&skip), 112);

        toggle(&entry, &mut skip, "data");
        assert!(skip.is_empty());

        toggle(&entry, &mut skip, "data/raw/a.bin");
        toggle(&entry, &mut skip, "data/raw/b.bin");
        assert_eq!(skip.iter().collect::<Vec<_>>(), ["data/raw"]);
        toggle(&entry, &mut skip, "data/notes.txt");
        assert_eq!(skip.iter().collect::<Vec<_>>(), ["data"]);
        toggle(&entry, &mut skip, "data");
        assert!(skip.is_empty());

        toggle(&entry, &mut skip, "");
        assert_eq!(entry.uploaded(&skip), 0);
        toggle(&entry, &mut skip, "src");
        assert_eq!(entry.uploaded(&skip), 5);
        fs::remove_dir_all(root).unwrap();
    }
}
