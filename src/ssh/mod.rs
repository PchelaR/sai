mod host_alias;
mod host_key;
mod key_login;
mod session;

use std::env;
use std::path::PathBuf;

use anyhow::Result;

pub use host_alias::install as install_host_alias;
pub use key_login::{is_setup_complete, verify as verify_key_login};
pub use session::{Auth, Env, Session};

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
    let installed = env::var_os("ProgramFiles").map(|dir| PathBuf::from(dir).join("OpenSSH"));
    let built_in =
        env::var_os("SystemRoot").map(|dir| PathBuf::from(dir).join("System32").join("OpenSSH"));
    [installed, built_in]
        .into_iter()
        .flatten()
        .find(|dir| dir.join("ssh.exe").is_file())
}

pub fn version(banner: &str) -> Option<(u32, u32)> {
    let rest = banner.split("OpenSSH_").nth(1)?;
    let rest = rest.strip_prefix("for_Windows_").unwrap_or(rest);
    let mut numbers = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().ok());
    Some((numbers.next()??, numbers.next()??))
}

pub fn is_unprotected_key(stderr: &str) -> bool {
    stderr.contains("UNPROTECTED PRIVATE KEY FILE") || stderr.contains("are too open")
}

fn dir() -> Result<PathBuf> {
    Ok(etcetera::home_dir()?.join(".ssh"))
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn reads_openssh_versions() {
        assert_eq!(
            version("OpenSSH_for_Windows_8.1p1, LibreSSL 3.0.2"),
            Some((8, 1))
        );
        assert_eq!(
            version("OpenSSH_for_Windows_9.5p1, LibreSSL 3.8.2"),
            Some((9, 5))
        );
        assert_eq!(version("OpenSSH_9.8p1, LibreSSL 3.3.6"), Some((9, 8)));
        assert_eq!(version("something else"), None);
    }
}
