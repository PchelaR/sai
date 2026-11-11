use std::{
    env::consts,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use etcetera::BaseStrategy;
use sha2::{Digest, Sha256};

use crate::output;

const VERSION: &str = "0.18.1";
const RELEASES: &str = "https://github.com/mutagen-io/mutagen/releases/download";
const PLATFORMS: &[(&str, &str, &str, &str)] = &[
    (
        "macos",
        "aarch64",
        "darwin_arm64",
        "6f810416d9e5fc4fd5e18431146f8b3c5a2056ba5a24f76c1e66da86eb3257e2",
    ),
    (
        "macos",
        "x86_64",
        "darwin_amd64",
        "7d06f7d8fcfe90bc7e55cc834a2f2f20c2e0af9ea9bc35911fc4341ad56a9bbf",
    ),
    (
        "linux",
        "x86_64",
        "linux_amd64",
        "7735286c778cc438418209f24d03a64f3a0151c8065ef0fe079cfaf093af6f8f",
    ),
    (
        "linux",
        "aarch64",
        "linux_arm64",
        "bcba735aebf8cbc11da9b3742118a665599ac697fa06bc5751cac8dcd540db8a",
    ),
    (
        "windows",
        "x86_64",
        "windows_amd64",
        "3e237e77f69959ed520a0f877330a431507bb0a85d9da7919764ba0c87b702c7",
    ),
];

pub fn binary() -> Result<PathBuf> {
    let dir = sai_data_dir()?.join(format!("mutagen-{VERSION}"));
    let binary = dir.join(executable());
    if !binary.exists() {
        install(&dir)?;
    }
    remove_old_versions()?;
    Ok(binary)
}

fn remove_old_versions() -> Result<()> {
    let data_dir = sai_data_dir()?;
    let Ok(entries) = fs::read_dir(&data_dir) else {
        return Ok(());
    };
    let current = format!("mutagen-{VERSION}");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&current) || !is_version_dir(&name) {
            continue;
        }
        let path = entry.path();
        stop_daemon(&path.join(executable()))?;
        if let Err(error) = fs::remove_dir_all(&path) {
            output::warning(&format!("failed to remove {}: {error}", path.display()));
        }
    }
    Ok(())
}

fn is_version_dir(name: &str) -> bool {
    name.strip_prefix("mutagen-")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|first| first.is_ascii_digit())
}

fn stop_daemon(binary: &Path) -> Result<()> {
    if binary.exists() {
        let _ = Command::new(binary)
            .args(["daemon", "stop"])
            .env("MUTAGEN_DATA_DIRECTORY", data_directory()?)
            .output();
    }
    Ok(())
}

fn executable() -> &'static str {
    if cfg!(windows) {
        "mutagen.exe"
    } else {
        "mutagen"
    }
}

pub fn data_directory() -> Result<PathBuf> {
    Ok(sai_data_dir()?.join("mutagen-data"))
}

fn sai_data_dir() -> Result<PathBuf> {
    Ok(etcetera::choose_base_strategy()?.data_dir().join("sai"))
}

fn install(dir: &Path) -> Result<()> {
    let (platform, checksum) = platform()?;
    output::step(&format!("Installing mutagen {VERSION}"));

    let staging = dir.with_file_name(format!("mutagen-{VERSION}.partial"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let archive = staging.join("mutagen.tar.gz");
    let url = format!("{RELEASES}/v{VERSION}/mutagen_{platform}_v{VERSION}.tar.gz");

    let downloaded = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&archive)
        .arg(&url)
        .status()
        .context("failed to run curl")?;
    ensure!(
        downloaded.success(),
        "failed to download mutagen from {url}"
    );
    verify(&archive, checksum)?;

    let extracted = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .status()
        .context("failed to run tar")?;
    ensure!(extracted.success(), "failed to unpack mutagen");
    fs::remove_file(&archive)?;
    fs::rename(&staging, dir)?;
    Ok(())
}

fn platform() -> Result<(&'static str, &'static str)> {
    PLATFORMS
        .iter()
        .find(|(os, arch, _, _)| *os == consts::OS && *arch == consts::ARCH)
        .map(|(_, _, platform, checksum)| (*platform, *checksum))
        .with_context(|| format!("sai does not support {}-{}", consts::OS, consts::ARCH))
}

fn verify(archive: &Path, expected: &str) -> Result<()> {
    let digest: String = Sha256::digest(fs::read(archive)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(
        digest == expected,
        "mutagen download has a wrong checksum, refusing to install it"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PLATFORMS, is_version_dir, platform};

    #[test]
    fn matches_only_version_dirs() {
        assert!(is_version_dir("mutagen-0.18.0"));
        assert!(is_version_dir("mutagen-0.17.6.partial"));
        assert!(!is_version_dir("mutagen-data"));
        assert!(!is_version_dir("mutagen"));
        assert!(!is_version_dir("snapshots"));
    }

    #[test]
    fn supports_the_current_platform() {
        assert!(platform().is_ok());
    }

    #[test]
    fn checksums_are_sha256() {
        for (_, _, _, checksum) in PLATFORMS {
            assert_eq!(checksum.len(), 64);
            assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
