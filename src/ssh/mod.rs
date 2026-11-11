mod host_alias;
mod host_key;
mod key_login;
mod session;

use std::env;
use std::path::PathBuf;

use anyhow::Result;

pub use host_alias::install as install_host_alias;
pub use key_login::{is_setup_complete, verify as verify_key_login};
pub use session::{Env, Session};

pub fn program(name: &str) -> PathBuf {
    match windows_openssh() {
        Some(dir) => dir.join(format!("{name}.exe")),
        None => PathBuf::from(name),
    }
}

pub fn windows_openssh() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let dir = PathBuf::from(env::var_os("SystemRoot")?)
        .join("System32")
        .join("OpenSSH");
    dir.join("ssh.exe").is_file().then_some(dir)
}

pub fn is_unprotected_key(stderr: &str) -> bool {
    stderr.contains("UNPROTECTED PRIVATE KEY FILE") || stderr.contains("are too open")
}

fn dir() -> Result<PathBuf> {
    Ok(etcetera::home_dir()?.join(".ssh"))
}
