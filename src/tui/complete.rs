use std::fs;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

pub struct Completion {
    pub value: String,
    pub options: Vec<String>,
}

pub fn folder(value: &str, home: Option<&Path>) -> Completion {
    let split = value.rfind(['/', '\\']).map_or(0, |index| index + 1);
    let (dir, prefix) = value.split_at(split);
    let base = resolve(dir, home);
    let mut names: Vec<String> = fs::read_dir(&base)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| prefix.starts_with('.') || !name.starts_with('.'))
                .filter(|name| starts_with(name, prefix))
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|name| name.to_lowercase());
    let separator = dir
        .chars()
        .rev()
        .find(|c| matches!(c, '/' | '\\'))
        .unwrap_or(MAIN_SEPARATOR);
    match names.as_slice() {
        [] => Completion {
            value: value.to_owned(),
            options: Vec::new(),
        },
        [single] => Completion {
            value: format!("{dir}{single}{separator}"),
            options: Vec::new(),
        },
        _ => Completion {
            value: format!("{dir}{}", common_prefix(&names, prefix)),
            options: names,
        },
    }
}

pub fn word(value: &str, candidates: &[&str]) -> Completion {
    let names: Vec<String> = candidates
        .iter()
        .filter(|name| name.starts_with(value))
        .map(|name| (*name).to_owned())
        .collect();
    match names.as_slice() {
        [single] => Completion {
            value: single.clone(),
            options: Vec::new(),
        },
        [] => Completion {
            value: value.to_owned(),
            options: Vec::new(),
        },
        _ => Completion {
            value: common_prefix(&names, value),
            options: names,
        },
    }
}

fn resolve(dir: &str, home: Option<&Path>) -> PathBuf {
    match (dir, home) {
        ("", _) => PathBuf::from("."),
        ("~/" | "~\\", Some(home)) => home.to_path_buf(),
        (dir, Some(home)) if dir.starts_with("~/") || dir.starts_with("~\\") => {
            home.join(&dir[2..])
        }
        (dir, _) => PathBuf::from(dir),
    }
}

fn starts_with(name: &str, prefix: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        name.to_lowercase().starts_with(&prefix.to_lowercase())
    } else {
        name.starts_with(prefix)
    }
}

fn common_prefix(names: &[String], typed: &str) -> String {
    let first = &names[0];
    let mut length = first.len();
    for name in &names[1..] {
        length = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, left), right)| {
                if cfg!(any(windows, target_os = "macos")) {
                    left.to_lowercase().eq(right.to_lowercase())
                } else {
                    left == right
                }
            })
            .map(|((index, character), _)| index + character.len_utf8())
            .last()
            .unwrap_or(0)
            .min(length);
    }
    if length < typed.len() {
        typed.to_owned()
    } else {
        first[..length].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{folder, word};

    fn sandbox() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("sai-complete-{}", std::process::id()));
        for dir in [
            "work/alpha",
            "work/beta-one",
            "work/beta-two",
            "work/.hidden",
            "other",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("work/bet.txt"), "").unwrap();
        root
    }

    #[test]
    fn completes_folders_like_a_shell() {
        let root = sandbox();
        let base = root.display().to_string();
        let one = folder(&format!("{base}/wo"), None);
        assert_eq!(one.value, format!("{base}/work/"));
        assert!(one.options.is_empty());
        let many = folder(&format!("{base}/work/be"), None);
        assert_eq!(many.value, format!("{base}/work/beta-"));
        assert_eq!(many.options, ["beta-one", "beta-two"]);
        let listing = folder(&format!("{base}/work/"), None);
        assert_eq!(listing.options, ["alpha", "beta-one", "beta-two"]);
        let hidden = folder(&format!("{base}/work/.h"), None);
        assert_eq!(hidden.value, format!("{base}/work/.hidden/"));
        let none = folder(&format!("{base}/work/zzz"), None);
        assert_eq!(none.value, format!("{base}/work/zzz"));
        let home = folder("~/wo", Some(Path::new(&base)));
        assert_eq!(home.value, "~/work/");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completes_project_names() {
        assert_eq!(word("sh", &["shop", "blog"]).value, "shop");
        let many = word("s", &["shop", "store", "blog"]);
        assert_eq!(many.value, "s");
        assert_eq!(many.options, ["shop", "store"]);
    }
}
