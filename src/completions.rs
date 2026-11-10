use std::io;

use anyhow::Result;
use clap_complete::Shell;

pub fn print(shell: Shell, mut command: clap::Command) -> Result<()> {
    let name = command.get_name().to_owned();
    clap_complete::generate(shell, &mut command, name, &mut io::stdout());
    Ok(())
}
