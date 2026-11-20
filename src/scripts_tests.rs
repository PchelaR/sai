//! Every script sai sends to the VPS must at least parse: a typo there shows up only on a
//! user's VPS otherwise
#![cfg(all(test, unix))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn scripts(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            scripts(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "sh") {
            found.push(path);
        }
    }
}

fn all() -> Vec<PathBuf> {
    let mut found = Vec::new();
    scripts(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    found.sort();
    found
}

#[test]
fn every_script_parses() {
    let found = all();
    assert!(found.len() > 40, "found only {} scripts", found.len());
    for script in found {
        let output = Command::new("bash")
            .arg("-n")
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} does not parse:\n{}",
            script.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn scripts_use_unix_line_endings() {
    for script in all() {
        let text = fs::read_to_string(&script).unwrap();
        assert!(!text.contains('\r'), "{} has CRLF", script.display());
    }
}
