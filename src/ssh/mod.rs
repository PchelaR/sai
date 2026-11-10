mod host_alias;
mod host_key;
mod key_login;
mod session;

use std::path::PathBuf;

use anyhow::Result;

pub use host_alias::install as install_host_alias;
pub use key_login::{is_setup_complete, verify as verify_key_login};
pub use session::{Env, Session};

fn dir() -> Result<PathBuf> {
    Ok(etcetera::home_dir()?.join(".ssh"))
}
