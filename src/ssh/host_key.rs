use std::{path::Path, process::Command};

use anyhow::{Result, bail};
use dialoguer::Confirm;
use russh::client;
use russh::keys::{self, HashAlg, PublicKey, PublicKeyOrCertificate};

use crate::mutagen;
use crate::vps::{HOST_ALIAS, SSH_PORT};

const PASSWORD_PORT: u16 = 22;

pub struct HostKeyPolicy {
    host: String,
}

impl HostKeyPolicy {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_owned(),
        }
    }
}

impl client::Handler for HostKeyPolicy {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let key = &key.public_key();
        let known_hosts = super::dir()?.join("known_hosts");
        if !known_hosts.exists() {
            return trust_on_first_use(&self.host, key, &known_hosts);
        }
        match keys::check_known_hosts_path(&self.host, PASSWORD_PORT, key, &known_hosts) {
            Ok(true) => Ok(true),
            Ok(false) => trust_on_first_use(&self.host, key, &known_hosts),
            Err(keys::Error::KeyChanged { .. }) => {
                replace_changed_key(&self.host, key, &known_hosts)
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn trust_on_first_use(host: &str, key: &PublicKey, known_hosts: &Path) -> Result<bool> {
    println!(
        "Host key fingerprint of {host}: {}",
        key.fingerprint(HashAlg::Sha256)
    );
    let trusted = Confirm::new()
        .with_prompt("Trust this host?")
        .default(false)
        .interact()?;
    if !trusted {
        bail!("host key of {host} was not trusted");
    }
    learn(host, key, known_hosts)
}

fn replace_changed_key(host: &str, key: &PublicKey, known_hosts: &Path) -> Result<bool> {
    println!(
        "Host key of {host} has changed, new fingerprint: {}",
        key.fingerprint(HashAlg::Sha256)
    );
    let reinstalled = Confirm::new()
        .with_prompt("Was the VPS reinstalled? sai will forget the old one and its syncs")
        .default(false)
        .interact()?;
    if !reinstalled {
        bail!("host key of {host} has changed, refusing to connect");
    }
    forget_old_vps(host)?;
    learn(host, key, known_hosts)
}

fn forget_old_vps(host: &str) -> Result<()> {
    for entry in [host.to_owned(), format!("[{host}]:{SSH_PORT}")] {
        Command::new("ssh-keygen").args(["-R", &entry]).output()?;
    }
    Command::new("ssh")
        .args(["-O", "exit", HOST_ALIAS])
        .output()?;
    mutagen::terminate_all()
}

fn learn(host: &str, key: &PublicKey, known_hosts: &Path) -> Result<bool> {
    for port in [PASSWORD_PORT, SSH_PORT] {
        keys::known_hosts::learn_known_hosts_path(host, port, key, known_hosts)?;
    }
    Ok(true)
}
