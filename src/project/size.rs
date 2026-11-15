use std::collections::BTreeSet;
use std::path::Path;

use super::content;

pub fn of(path: &Path, skip: &[String]) -> u64 {
    let skip: BTreeSet<String> = skip.iter().cloned().collect();
    content::scan(path).uploaded(&skip)
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
        for dir in [
            "src",
            "data",
            "node_modules/x",
            ".git",
            "target",
            "venv",
            "app.egg-info",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("src/a"), [0u8; 100]).unwrap();
        fs::write(root.join("data/big"), [0u8; 500]).unwrap();
        fs::write(root.join("node_modules/x/b"), [0u8; 1000]).unwrap();
        fs::write(root.join(".git/c"), [0u8; 1000]).unwrap();
        fs::write(root.join("target/d"), [0u8; 1000]).unwrap();
        fs::write(root.join("venv/e"), [0u8; 1000]).unwrap();
        fs::write(root.join("app.egg-info/f"), [0u8; 1000]).unwrap();
        assert_eq!(of(&root, &[]), 600);
        assert_eq!(of(&root, &["data".to_owned()]), 100);
        fs::remove_dir_all(&root).unwrap();
    }
}
