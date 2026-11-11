use std::env;

use anyhow::{Result, ensure};

pub fn ensure_supported() -> Result<()> {
    ensure!(
        !is_mintty(),
        "this terminal window (mintty from Git Bash or MSYS2) cannot run interactive programs, open sai in Windows Terminal or PowerShell"
    );
    Ok(())
}

pub fn is_mintty() -> bool {
    cfg!(windows) && env::var("TERM_PROGRAM").is_ok_and(|program| program == "mintty")
}
