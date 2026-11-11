use std::fmt::Write;
use std::{env, fs};

use anyhow::Result;

use super::{dir, mask, recent, time};
use crate::{doctor, mutagen, output};

const LOG_LINES: usize = 500;

pub fn run() -> Result<()> {
    let path = env::current_dir()?.join(format!("sai-report-{}.txt", time::stamp()));
    fs::write(&path, mask(&collect()))?;
    output::info(&format!("Report saved to {}", path.display()));
    output::info("Send this file to whoever helps you with sai");
    Ok(())
}

fn collect() -> String {
    let mut report = String::new();
    writeln!(report, "sai {}", env!("CARGO_PKG_VERSION")).ok();
    writeln!(report, "platform {}-{}", env::consts::OS, env::consts::ARCH).ok();
    if let Ok(executable) = env::current_exe() {
        writeln!(report, "executable {}", executable.display()).ok();
    }
    if let Ok(dir) = dir() {
        writeln!(report, "logs {}", dir.display()).ok();
    }

    section(&mut report, "doctor");
    let (result, lines) = output::collect(doctor::run);
    for line in lines {
        writeln!(report, "{line}").ok();
    }
    if let Err(error) = result {
        writeln!(report, "Error: {error:#}").ok();
    }

    section(&mut report, "sync");
    match mutagen::describe() {
        Ok(text) => report.push_str(&text),
        Err(error) => {
            writeln!(report, "Error: {error:#}").ok();
        }
    }

    section(&mut report, &format!("last {LOG_LINES} log lines"));
    for line in recent(LOG_LINES) {
        writeln!(report, "{line}").ok();
    }
    report
}

fn section(report: &mut String, title: &str) {
    writeln!(report, "\n== {title}").ok();
}
