mod release;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use etcetera::BaseStrategy;
use sha2::{Digest, Sha256};

use clap::ValueEnum;

use crate::agent::Agent;
use crate::{image, output};

const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_FILE: &str = "latest-release";
const SUMS: &str = "SHA256SUMS";
const OLD_SUFFIX: &str = ".old";

pub fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let executable = executable()?;
    output::step("Looking for a new version of sai");
    let release = release::latest(release::SLOW)?;
    remember(&release.version);
    if !newer(&release.version, current) {
        output::info(&format!("sai {current} is the latest version"));
        return Ok(());
    }
    let target = target()?;
    let archive = release.archive_name(target);
    let archive_url = release.asset(&archive)?;
    let sums_url = release.asset(SUMS)?;

    let work = data_dir()?.join(format!("upgrade-{}", release.version));
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;
    output::step(&format!("Downloading sai {}", release.version));
    download(&work, &archive_url, &archive)?;
    download(&work, &sums_url, SUMS)?;
    verify(&work, &archive)?;
    extract(&work, &archive)?;

    let binary = work.join(archive_folder(&archive)).join(executable_name());
    ensure!(binary.is_file(), "the downloaded archive has no sai inside");
    let rebuild = rebuild_notice(&binary);
    replace(&executable, &binary)?;
    fs::remove_dir_all(&work).ok();
    output::info(&format!(
        "Updated sai {current} to {}. Restart sai to use it",
        release.version
    ));
    if let Some(rebuild) = rebuild {
        output::warning(rebuild);
    }
    Ok(())
}

/// Asks the new sai which images it runs: a changed image is rebuilt on the VPS by the
/// next `sai chat`, and the user should know before that chat takes half an hour.
/// Versions without this check answer nothing, and nothing is said
fn rebuild_notice(binary: &Path) -> Option<&'static str> {
    let output = Command::new(binary).arg("image-tags").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let tags = String::from_utf8_lossy(&output.stdout);
    let mut tags = tags.lines().map(str::trim);
    if tags.next()? != image::base() {
        return Some(
            "the new version changes the agent image: the next `sai chat` rebuilds it on the VPS once, this takes 15-30 minutes. Projects, files and chats stay as they are",
        );
    }
    let agents: Vec<String> = Agent::value_variants()
        .iter()
        .map(|agent| image::agent(*agent))
        .collect();
    tags.any(|tag| !agents.iter().any(|agent| agent == tag))
        .then_some("the new version changes the agent image: the next `sai chat` rebuilds it on the VPS once, this takes 2-5 minutes")
}

pub fn available() -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = match cached() {
        Some(latest) => latest,
        None => {
            let latest = release::latest(release::QUICK)
                .map_or_else(|_| current.to_owned(), |release| release.version);
            remember(&latest);
            latest
        }
    };
    newer(&latest, current).then_some(latest)
}

pub fn notice() {
    if let Some(version) = available() {
        output::warning(&format!(
            "sai {version} is available, update with `sai upgrade`"
        ));
    }
}

pub fn clean_up() {
    if let Ok(executable) = env::current_exe() {
        let old = old_path(&executable);
        if old.exists() {
            fs::remove_file(old).ok();
        }
    }
}

fn executable() -> Result<PathBuf> {
    let executable = fs::canonicalize(env::current_exe()?)?;
    let in_target = executable
        .components()
        .any(|component| component.as_os_str() == "target");
    ensure!(
        !in_target,
        "this sai was built from source ({}), update it with git and cargo instead",
        executable.display()
    );
    Ok(executable)
}

fn target() -> Result<&'static str> {
    Ok(match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        (os, arch) => bail!("there is no ready-made sai for {os}-{arch}, build it from source"),
    })
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "sai.exe" } else { "sai" }
}

fn archive_folder(archive: &str) -> &str {
    archive
        .strip_suffix(".tar.gz")
        .or_else(|| archive.strip_suffix(".zip"))
        .unwrap_or(archive)
}

fn download(dir: &Path, url: &str, name: &str) -> Result<()> {
    let output = Command::new("curl")
        .current_dir(dir)
        .args(["-fsSL", "--max-time", "300", "-o", name, url])
        .output()
        .context("failed to run curl")?;
    ensure!(
        output.status.success(),
        "failed to download {url}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn verify(dir: &Path, archive: &str) -> Result<()> {
    let sums = fs::read_to_string(dir.join(SUMS))?;
    let expected = sums
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, name)| name.trim().trim_start_matches('*') == archive)
        .map(|(sum, _)| sum.to_lowercase())
        .with_context(|| format!("{SUMS} of the release does not list {archive}"))?;
    let actual: String = Sha256::digest(fs::read(dir.join(archive))?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(
        actual == expected,
        "the downloaded sai has a wrong checksum, refusing to install it"
    );
    Ok(())
}

fn extract(dir: &Path, archive: &str) -> Result<()> {
    let tar = match env::var_os("SystemRoot") {
        Some(root) if cfg!(windows) => PathBuf::from(root).join("System32").join("tar.exe"),
        _ => PathBuf::from("tar"),
    };
    let output = Command::new(tar)
        .current_dir(dir)
        .args(["-xf", archive])
        .output()
        .context("failed to run tar")?;
    ensure!(
        output.status.success(),
        "failed to unpack {archive}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn replace(executable: &Path, binary: &Path) -> Result<()> {
    let dir = executable
        .parent()
        .context("cannot find the folder of sai")?;
    let staged = dir.join(format!(".{}.new", executable_name()));
    fs::copy(binary, &staged).map_err(|error| permission_hint(dir, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    }
    if cfg!(windows) {
        let old = old_path(executable);
        if old.exists() {
            fs::remove_file(&old).ok();
        }
        fs::rename(executable, &old).map_err(|error| permission_hint(dir, error))?;
        if let Err(error) = fs::rename(&staged, executable) {
            fs::rename(&old, executable).ok();
            return Err(permission_hint(dir, error));
        }
    } else {
        fs::rename(&staged, executable).map_err(|error| permission_hint(dir, error))?;
    }
    Ok(())
}

fn permission_hint(dir: &Path, error: std::io::Error) -> anyhow::Error {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        let how = if cfg!(windows) {
            "run the terminal as administrator and try again"
        } else {
            "run `sudo sai upgrade`"
        };
        anyhow::anyhow!(
            "sai is installed in {}, which needs administrator rights: {how}",
            dir.display()
        )
    } else {
        anyhow::Error::new(error).context("failed to replace sai")
    }
}

fn old_path(executable: &Path) -> PathBuf {
    let mut name = executable.as_os_str().to_owned();
    name.push(OLD_SUFFIX);
    PathBuf::from(name)
}

fn newer(candidate: &str, current: &str) -> bool {
    parse(candidate) > parse(current)
}

fn parse(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn data_dir() -> Result<PathBuf> {
    Ok(etcetera::choose_base_strategy()?.data_dir().join("sai"))
}

fn cached() -> Option<String> {
    let text = fs::read_to_string(data_dir().ok()?.join(CHECK_FILE)).ok()?;
    let (checked, version) = text.trim().split_once(' ')?;
    let checked: u64 = checked.parse().ok()?;
    (now().saturating_sub(checked) < CHECK_EVERY.as_secs()).then(|| version.to_owned())
}

fn remember(version: &str) {
    if let Ok(dir) = data_dir() {
        fs::create_dir_all(&dir).ok();
        fs::write(dir.join(CHECK_FILE), format!("{} {version}", now())).ok();
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{SUMS, archive_folder, newer, verify};

    #[test]
    fn compares_versions() {
        assert!(newer("v0.1.8", "0.1.7"));
        assert!(newer("0.2.0", "0.1.12"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("v0.1.7", "0.1.7"));
        assert!(!newer("0.1.6", "0.1.7"));
    }

    #[test]
    fn finds_the_folder_inside_archives() {
        assert_eq!(
            archive_folder("sai-v0.1.8-aarch64-apple-darwin.tar.gz"),
            "sai-v0.1.8-aarch64-apple-darwin"
        );
        assert_eq!(
            archive_folder("sai-v0.1.8-x86_64-pc-windows-msvc.zip"),
            "sai-v0.1.8-x86_64-pc-windows-msvc"
        );
    }

    #[test]
    fn checks_archives_against_sums() {
        let dir = std::env::temp_dir().join(format!("sai-verify-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("sai.tar.gz"), "hello\n").unwrap();
        let sum = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
        fs::write(dir.join(SUMS), format!("{sum}  sai.tar.gz\n")).unwrap();
        assert!(verify(&dir, "sai.tar.gz").is_ok());
        fs::write(dir.join(SUMS), format!("{sum}  other.tar.gz\n")).unwrap();
        assert!(verify(&dir, "sai.tar.gz").is_err());
        fs::write(dir.join("sai.tar.gz"), "changed\n").unwrap();
        fs::write(dir.join(SUMS), format!("{sum} *sai.tar.gz\n")).unwrap();
        assert!(verify(&dir, "sai.tar.gz").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
