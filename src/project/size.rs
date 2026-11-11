use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::{ignores, secrets};

const LARGE: u64 = 1024 * 1024 * 1024;
const VCS: &[&str] = &[".git", ".hg", ".svn"];

pub fn is_large(bytes: u64) -> bool {
    bytes > LARGE
}

pub fn of(path: &Path) -> u64 {
    let skipped: HashSet<&str> = ignores::BUILD
        .iter()
        .chain(secrets::PATTERNS)
        .filter_map(|pattern| pattern.strip_suffix('/'))
        .chain(VCS.iter().copied())
        .collect();
    let mut total = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !skipped.contains(entry.file_name().to_string_lossy().as_ref()) {
                    pending.push(entry.path());
                }
            } else if kind.is_file() {
                total += entry.metadata().map_or(0, |metadata| metadata.len());
            }
        }
    }
    total
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{human, of};

    #[test]
    fn formats_sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn skips_what_is_not_synced() {
        let root = std::env::temp_dir().join(format!("sai-size-{}", std::process::id()));
        for dir in ["src", "node_modules/x", ".git", "target"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("src/a"), [0u8; 100]).unwrap();
        fs::write(root.join("node_modules/x/b"), [0u8; 1000]).unwrap();
        fs::write(root.join(".git/c"), [0u8; 1000]).unwrap();
        fs::write(root.join("target/d"), [0u8; 1000]).unwrap();
        assert_eq!(of(&root), 100);
        fs::remove_dir_all(&root).unwrap();
    }
}
