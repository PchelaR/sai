mod report;
mod time;

use std::env::consts;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::Result;
use etcetera::BaseStrategy;

use crate::config::Config;

pub use report::run as report;

const FILE: &str = "sai.log";
const ROTATED: &str = "sai.log.1";
const MAX_SIZE: u64 = 1024 * 1024;
const MAX_ARG_LENGTH: usize = 120;
const VPS: &str = "<vps>";
const HOME: &str = "~";

pub fn start(command: &str) {
    rotate();
    write(&format!(
        "=== sai {} {}-{} {command}",
        env!("CARGO_PKG_VERSION"),
        consts::OS,
        consts::ARCH
    ));
}

pub fn line(text: &str) {
    write(text);
}

pub fn error(error: &anyhow::Error) {
    write(&format!("Error: {error:#}"));
}

pub fn describe<S: AsRef<OsStr>>(args: &[S]) -> String {
    args.iter()
        .map(|arg| {
            let arg = arg.as_ref().to_string_lossy();
            if arg.chars().count() > MAX_ARG_LENGTH {
                format!("<{} chars>", arg.chars().count())
            } else {
                arg.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn mask(text: &str) -> String {
    let (ip, home) = private();
    let mut text = text.to_owned();
    if let Some(ip) = ip {
        text = text.replace(ip.as_str(), VPS);
    }
    if let Some(home) = home {
        text = text.replace(home.as_str(), HOME);
    }
    text
}

pub fn dir() -> Result<PathBuf> {
    Ok(etcetera::choose_base_strategy()?
        .data_dir()
        .join("sai")
        .join("logs"))
}

fn recent(limit: usize) -> Vec<String> {
    let Ok(dir) = dir() else {
        return Vec::new();
    };
    let lines: Vec<String> = [ROTATED, FILE]
        .iter()
        .filter_map(|name| fs::read_to_string(dir.join(name)).ok())
        .flat_map(|content| content.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    lines[lines.len().saturating_sub(limit)..].to_vec()
}

fn write(text: &str) {
    let Ok(dir) = dir() else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE))
    else {
        return;
    };
    let stamp = time::now();
    for line in mask(text).lines() {
        writeln!(file, "{stamp} {line}").ok();
    }
}

fn rotate() {
    let Ok(dir) = dir() else {
        return;
    };
    let current = dir.join(FILE);
    if fs::metadata(&current).is_ok_and(|metadata| metadata.len() > MAX_SIZE) {
        fs::rename(&current, dir.join(ROTATED)).ok();
    }
}

fn private() -> &'static (Option<String>, Option<String>) {
    static PRIVATE: OnceLock<(Option<String>, Option<String>)> = OnceLock::new();
    PRIVATE.get_or_init(|| {
        let ip = Config::load()
            .ok()
            .map(|config| config.ip)
            .filter(|ip| !ip.is_empty());
        let home = etcetera::home_dir()
            .ok()
            .map(|home| home.display().to_string())
            .filter(|home| home.len() > 1);
        (ip, home)
    })
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn shortens_long_arguments() {
        let long = "x".repeat(500);
        assert_eq!(describe(&["sync", long.as_str()]), "sync <500 chars>");
    }
}
