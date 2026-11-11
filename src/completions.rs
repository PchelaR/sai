use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap_complete::Shell;
use etcetera::BaseStrategy;

use crate::{files, logs, output};

const BEGIN: &str = "# BEGIN sai";
const END: &str = "# END sai";

pub fn print(shell: Shell, mut command: clap::Command) -> Result<()> {
    let name = command.get_name().to_owned();
    clap_complete::generate(shell, &mut command, name, &mut io::stdout());
    Ok(())
}

pub fn install_quietly(command: clap::Command) {
    match install(command) {
        Ok(true) => {
            output::info("Tab completion for sai is installed, it works in new terminal windows")
        }
        Ok(false) => {}
        Err(error) => logs::line(&format!("completion install skipped: {error:#}")),
    }
}

fn install(command: clap::Command) -> Result<bool> {
    let Some(shell) = current_shell() else {
        return Ok(false);
    };
    let home = home()?;
    let data = etcetera::choose_base_strategy()?
        .data_dir()
        .join("sai")
        .join("completions");
    let (script, rc) = match shell {
        Shell::Zsh => (data.join("zsh").join("_sai"), Some(home.join(".zshrc"))),
        Shell::Bash => (data.join("sai.bash"), Some(home.join(".bashrc"))),
        Shell::Fish => (home.join(".config/fish/completions/sai.fish"), None),
        _ => return Ok(false),
    };
    let fresh = !script.exists();
    write_if_changed(&script, &generate(shell, command))?;
    let Some(rc) = rc else {
        return Ok(fresh);
    };
    let block = rc_block(shell, &script);
    let existing = files::read_or_empty(&rc)?;
    if existing.contains(&block) {
        return Ok(false);
    }
    let kept = without_block(&existing);
    let separator = if kept.is_empty() || kept.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(&rc, format!("{kept}{separator}{block}"))
        .with_context(|| format!("failed to update {}", rc.display()))?;
    Ok(true)
}

fn current_shell() -> Option<Shell> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell)
        .file_stem()?
        .to_string_lossy()
        .into_owned();
    match name.as_str() {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        "fish" => Some(Shell::Fish),
        _ => None,
    }
}

fn home() -> Result<PathBuf> {
    match env::var_os("HOME") {
        Some(home) if Path::new(&home).is_absolute() => Ok(PathBuf::from(home)),
        _ => Ok(etcetera::home_dir()?),
    }
}

fn generate(shell: Shell, mut command: clap::Command) -> String {
    let name = command.get_name().to_owned();
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut command, name, &mut script);
    String::from_utf8_lossy(&script).into_owned()
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if files::read_or_empty(path)? == content {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn rc_block(shell: Shell, script: &Path) -> String {
    let script = script.display().to_string().replace('\\', "/");
    let body = match shell {
        Shell::Zsh => {
            let dir = script.trim_end_matches("/_sai");
            format!("fpath=(\"{dir}\" $fpath)\nautoload -Uz compinit && compinit")
        }
        _ => format!("[ -f \"{script}\" ] && . \"{script}\""),
    };
    format!("{BEGIN}\n{body}\n{END}\n")
}

fn without_block(content: &str) -> String {
    let mut inside = false;
    content
        .lines()
        .filter(|line| match line.trim() {
            BEGIN => {
                inside = true;
                false
            }
            END => {
                inside = false;
                false
            }
            _ => !inside,
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap_complete::Shell;

    use super::{rc_block, without_block};

    #[test]
    fn replaces_only_its_own_block() {
        let block = rc_block(Shell::Bash, Path::new("/data/sai.bash"));
        let content = format!("export A=1\n{block}alias ll='ls -l'\n");
        assert_eq!(without_block(&content), "export A=1\nalias ll='ls -l'\n");
    }

    #[test]
    fn zsh_block_adds_the_completion_dir() {
        let block = rc_block(Shell::Zsh, Path::new("/data/zsh/_sai"));
        assert!(block.contains("fpath=(\"/data/zsh\" $fpath)"));
        assert!(block.starts_with("# BEGIN sai\n") && block.ends_with("# END sai\n"));
    }
}
