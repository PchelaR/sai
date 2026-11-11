mod report;

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};

use crate::config::Config;
use crate::{console, image, mutagen, remote, ssh};
use report::Report;

const VPS_CHECKS: &str = include_str!("scripts/vps_checks.sh");

pub fn run() -> Result<()> {
    let mut report = Report::default();

    report.section("Local");
    let config = report.check("config", Config::load());
    if let Some(config) = &config {
        report.check("ssh key", config.public_key());
    }
    report.check("ssh client", installed(&ssh::program("ssh"), "-V"));
    report.check("terminal", console::ensure_supported());
    report.check("mutagen", mutagen::ensure_installed());

    let Some(config) = config else {
        return report.finish();
    };
    report.section(&format!("VPS {}", config.ip));
    let login = ssh::verify_key_login(&config)
        .context("the VPS is unreachable or not set up yet, run `sai setup`");
    if report.check("login as sai", login).is_none() {
        return report.finish();
    }
    let setup = if ssh::is_setup_complete(&config) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "setup was interrupted, run `sai setup` again"
        ))
    };
    report.check("setup finished", setup);
    check_vps(&mut report)?;

    report.section("Sync");
    check_sync(&mut report)?;

    report.finish()
}

fn installed(program: &Path, version_flag: &str) -> Result<()> {
    let output = Command::new(program)
        .arg(version_flag)
        .output()
        .with_context(|| format!("{} is not installed", program.display()))?;
    ensure!(
        output.status.success(),
        "{} does not work",
        program.display()
    );
    Ok(())
}

fn check_vps(report: &mut Report) -> Result<()> {
    let output = remote::output(VPS_CHECKS, &[&image::base()])?;
    for line in output.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(status), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        let detail = fields.next().unwrap_or_default();
        match status {
            "ok" => report.ok(name),
            "warn" => report.warn(name, detail),
            _ => report.fail(name, detail),
        }
    }
    Ok(())
}

fn check_sync(report: &mut Report) -> Result<()> {
    let sessions = mutagen::sessions()?;
    if sessions.is_empty() {
        report.ok("no projects attached yet");
    }
    for session in sessions {
        let issues = session.issues();
        if issues.is_empty() {
            report.ok(&format!("{}: {}", session.name, session.summary()));
        }
        for issue in issues {
            report.fail(&session.name, issue);
        }
    }
    Ok(())
}
