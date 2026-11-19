mod report;

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};

use crate::config::Config;
use crate::project::Registry;
use crate::vps::SETUP_MARKER;
use crate::{console, image, mutagen, remote, ssh, windows_terminal};
use report::Report;

const MIN_WINDOWS_SSH: (u32, u32) = (9, 0);
const VPS_CHECKS: &str = include_str!("scripts/vps_checks.sh");

pub fn run() -> Result<()> {
    let mut report = Report::default();

    report.section("Local");
    let config = report.check("config", Config::load());
    if let Some(config) = &config {
        report.check("ssh key", config.public_key());
        check_permissions(&report, config);
    }
    report.check("ssh client", installed(&ssh::program("ssh"), "-V"));
    check_windows_ssh(&mut report);
    report.check("terminal", console::ensure_supported());
    if console::is_msys() {
        report.warn("terminal", console::MSYS_ADVICE);
    }
    if let Some(settings) = windows_terminal::settings() {
        let tips = windows_terminal::tips(&settings);
        if tips.is_empty() {
            report.ok("windows terminal");
        }
        for tip in tips {
            report.warn("windows terminal", tip);
        }
    }
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

fn check_permissions(report: &Report, config: &Config) {
    let lifted = config.agents.lifted();
    if lifted.is_empty() {
        report.ok("agent restrictions");
    } else {
        report.warn(
            "agent restrictions",
            format!(
                "the config lets agents run {}, only snapshots can bring back what they break",
                lifted.join(", ")
            ),
        );
    }
}

fn check_windows_ssh(report: &mut Report) {
    if !cfg!(windows) {
        return;
    }
    let Ok(output) = Command::new(ssh::program("ssh")).arg("-V").output() else {
        return;
    };
    let banner = String::from_utf8_lossy(&output.stderr);
    if let Some(version @ (major, minor)) = ssh::version(&banner)
        && version < MIN_WINDOWS_SSH
    {
        report.warn(
            "ssh version",
            format!(
                "OpenSSH {major}.{minor} on this PC has known terminal bugs, install the latest one from https://github.com/PowerShell/Win32-OpenSSH/releases and sai will use it"
            ),
        );
    }
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
    let output = remote::output(VPS_CHECKS, &[&image::base(), SETUP_MARKER])?;
    let registry = Registry::load()?;
    let mut leftovers = Vec::new();
    for line in output.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(status), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        let detail = fields.next().unwrap_or_default();
        match status {
            "project" if registry.find(name).is_none() => leftovers.push(name),
            "project" => {}
            "ok" => report.ok(name),
            "warn" => report.warn(name, detail),
            _ => report.fail(name, detail),
        }
    }
    if !leftovers.is_empty() {
        report.warn(
            "files of removed projects",
            format!(
                "{} still take space on the VPS, delete them with `sai rm <name> --purge`",
                leftovers.join(", ")
            ),
        );
    }
    Ok(())
}

fn check_sync(report: &mut Report) -> Result<()> {
    let sessions = mutagen::sessions()?;
    if sessions.is_empty() {
        report.ok("no projects attached yet");
    }
    for session in sessions {
        let name = session
            .name
            .trim_start_matches("sai-")
            .replacen("--", "/", 1);
        let issues = session.issues();
        if issues.is_empty() {
            report.ok(&format!("{name}: {}", session.summary()));
        }
        for issue in issues {
            report.fail(&name, issue);
        }
    }
    Ok(())
}
