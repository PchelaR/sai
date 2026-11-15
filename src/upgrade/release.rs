use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

const LATEST: &str = "https://api.github.com/repos/PchelaR/sai/releases/latest";
pub const QUICK: &str = "3";
pub const SLOW: &str = "30";

#[derive(Deserialize)]
struct Payload {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct Release {
    pub version: String,
    tag: String,
    assets: Vec<Asset>,
}

impl Release {
    pub fn archive_name(&self, target: &str) -> String {
        let extension = if target.contains("windows") {
            "zip"
        } else {
            "tar.gz"
        };
        format!("sai-{}-{target}.{extension}", self.tag)
    }

    pub fn asset(&self, name: &str) -> Result<String> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
            .with_context(|| format!("release {} has no {name}", self.tag))
    }
}

pub fn latest(timeout: &str) -> Result<Release> {
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", timeout])
        .args(["-H", "Accept: application/vnd.github+json"])
        .arg(LATEST)
        .output()
        .context("failed to run curl")?;
    ensure!(
        output.status.success(),
        "could not reach GitHub to check for a new version of sai"
    );
    parse(&String::from_utf8_lossy(&output.stdout))
}

fn parse(json: &str) -> Result<Release> {
    let payload: Payload =
        serde_json::from_str(json).context("unexpected answer from GitHub releases")?;
    Ok(Release {
        version: payload.tag_name.trim_start_matches('v').to_owned(),
        tag: payload.tag_name,
        assets: payload.assets,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_github_releases() {
        let release = parse(
            r#"{"tag_name":"v0.1.8","assets":[{"name":"sai-v0.1.8-aarch64-apple-darwin.tar.gz","browser_download_url":"https://x/a.tar.gz"},{"name":"SHA256SUMS","browser_download_url":"https://x/sums"}]}"#,
        )
        .unwrap();
        assert_eq!(release.version, "0.1.8");
        assert_eq!(
            release.archive_name("aarch64-apple-darwin"),
            "sai-v0.1.8-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(release.asset("SHA256SUMS").unwrap(), "https://x/sums");
        assert!(release.asset("missing").is_err());
    }
}
