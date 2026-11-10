use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::agent::{Image, Policy};
use crate::vps::USER;
use crate::{ignores, remote, secrets};

const INSTALL: &str = include_str!("install.sh");
const SNAPSHOTS: &str = include_str!("snapshots.sh");
const SNAPSHOT_SERVICE: &str = include_str!("sai-snapshot.service");
const SNAPSHOT_TIMER: &str = include_str!("sai-snapshot.timer");
const EXECUTABLE: &str = "755";
const READABLE: &str = "644";

pub fn snapshots_path() -> String {
    format!("{}/bin/snapshots", dir())
}

pub fn policy_path(policy: &Policy) -> String {
    format!("{}/{}", dir(), policy.name)
}

pub fn install(image: &Image) -> Result<()> {
    let units = format!("/home/{USER}/.config/systemd/user");
    let files = [
        (snapshots_path(), EXECUTABLE, SNAPSHOTS.to_owned()),
        (
            format!("{}/snapshot-excludes", dir()),
            READABLE,
            snapshot_excludes(),
        ),
        (
            format!("{units}/sai-snapshot.service"),
            READABLE,
            SNAPSHOT_SERVICE.to_owned(),
        ),
        (
            format!("{units}/sai-snapshot.timer"),
            READABLE,
            SNAPSHOT_TIMER.to_owned(),
        ),
        (
            policy_path(&image.policy),
            READABLE,
            image.policy.content.to_owned(),
        ),
    ];
    let args: Vec<String> = files
        .into_iter()
        .flat_map(|(path, mode, content)| [path, mode.to_owned(), BASE64_STANDARD.encode(content)])
        .collect();
    remote::run(
        INSTALL,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

fn dir() -> String {
    format!("/home/{USER}/.sai")
}

fn snapshot_excludes() -> String {
    ignores::BUILD
        .iter()
        .chain(secrets::PATTERNS)
        .map(|pattern| format!("{pattern}\n"))
        .collect()
}
