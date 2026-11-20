//! Helpers for tests that run sai's VPS scripts on this machine. Scripts go through the
//! same composition as on the VPS (arguments, `set -euo pipefail`), with HOME pointing
//! at a temporary folder. Tests that need Docker run only when SAI_DOCKER_TESTS is set:
//! CI does that on Linux, where Docker is there and nothing else uses it
// Script tests run on Unix only, Windows builds the helpers without using them
#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let path = env::temp_dir().join(format!(
            "sai-test-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).ok();
    }
}

/// A unique lowercase name for things tests create in Docker
pub fn unique(label: &str) -> String {
    format!(
        "{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn has(program: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Docker tests change containers and volumes, so they never run by accident
pub fn docker() -> bool {
    env::var_os("SAI_DOCKER_TESTS").is_some() && has("docker")
}

pub fn run(script: &str, args: &[&str], home: &Path) -> Output {
    crate::remote::run_locally(script, args, home)
}

pub fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The output of a script that must succeed, with its errors in the panic message
#[track_caller]
pub fn ok(output: Output) -> String {
    assert!(
        output.status.success(),
        "script failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text(&output)
}

#[track_caller]
pub fn fails(output: Output) -> String {
    assert!(
        !output.status.success(),
        "script should have failed\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Runs a command on this machine and returns its stdout, whatever the status
pub fn shell(command: &str) -> String {
    let output = Command::new("bash").args(["-c", command]).output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
