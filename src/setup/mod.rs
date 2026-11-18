mod credentials;
mod steps;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::config::Config;
use crate::ssh::{self, Env, Session};
use crate::vps::{self, HOST_ALIAS, MINIMAL, SETUP_MARKER, SSH_PORT, USER};
use crate::{image, output};
use steps::Step;

const FRESH_SSH_PORT: u16 = 22;

pub async fn run(minimal: bool) -> Result<()> {
    let config = Config::load()?;
    check_port(&config, minimal)?;
    if ssh::is_setup_complete(&config) {
        ssh::install_host_alias(&config)?;
        output::info(&format!(
            "{} is already configured. Connect with: ssh {HOST_ALIAS}",
            config.ip
        ));
        return Ok(());
    }

    let mut env = script_env(&config)?;
    let credentials = credentials::ask(&config.ip)?;
    if !confirmed(&config, minimal)? {
        output::info("Nothing was changed");
        return Ok(());
    }
    let keep_cache = Confirm::new()
        .with_prompt(
            "Keep Docker's build cache on the VPS? Rebuilding the agent image after a sai update then takes minutes instead of 15-30, but the cache takes a few more GB of disk. Worth it with 50 GB of disk or more",
        )
        .default(false)
        .interact()?;
    env.push((
        "SAI_KEEP_BUILD_CACHE",
        if keep_cache { "yes" } else { "" }.to_owned(),
    ));
    let login_port = if minimal {
        config.port()
    } else {
        FRESH_SSH_PORT
    };
    let session =
        Session::connect(&config, login_port, &credentials.user, credentials.auth).await?;

    if minimal {
        run_steps(&session, steps::MINIMAL, &env).await?;
        ssh::verify_key_login(&config).with_context(|| {
            format!(
                "key login as {USER} on port {} failed, check that sshd allows it",
                config.port()
            )
        })?;
    } else {
        run_steps(&session, steps::PREPARE, &env).await?;
        if let Err(error) = lock_down(&session, &config, &env).await {
            run_steps(&session, steps::ROLLBACK, &env).await?;
            return Err(error.context("ssh lockdown failed, ssh changes were rolled back"));
        }
    }

    ssh::install_host_alias(&config)?;
    print_summary(&session, &env).await?;
    output::info(&format!(
        "Done. {} is configured. Next, open an agent in a project folder: cd <folder> && sai chat",
        config.ip
    ));
    Ok(())
}

async fn lock_down(session: &Session, config: &Config, env: &Env<'_>) -> Result<()> {
    ssh::verify_key_login(config)
        .with_context(|| format!("key login as {USER} on port {SSH_PORT} failed"))?;
    run_steps(session, steps::LOCK_DOWN, env).await?;
    ssh::verify_key_login(config).context("key login failed after lockdown")
}

fn confirmed(config: &Config, minimal: bool) -> Result<bool> {
    output::info(&plan(config, minimal));
    Ok(Confirm::new()
        .with_prompt("Do you agree to these changes?")
        .default(false)
        .interact()?)
}

fn plan(config: &Config, minimal: bool) -> String {
    if minimal {
        format!(
            "sai will set up {ip} without touching ssh, the firewall or fail2ban:\n\
             \x20 - create the user {USER} without sudo, it logs in with your ssh key on port {port}\n\
             \x20 - install Docker and an isolated network for agents\n\
             \x20 - add swap if there is none\n\
             \x20 - build the base image for agents (up to an hour)",
            ip = config.ip,
            port = config.port(),
        )
    } else {
        format!(
            "sai will secure {ip} and prepare it for agents:\n\
             \x20 - create the user {USER} without sudo, it logs in only with your ssh key\n\
             \x20 - move ssh from port 22 to {SSH_PORT}\n\
             \x20 - turn off password login and login as root\n\
             \x20 - turn on the ufw firewall, only port {SSH_PORT} stays open\n\
             \x20 - turn on fail2ban: 3 failed logins ban the IP for 1 hour\n\
             \x20 - update the system and turn on automatic security updates\n\
             \x20 - install Docker and build the base image for agents (up to an hour)\n\
             After this the VPS is reachable only with your ssh key (sai or `ssh {HOST_ALIAS}`),\n\
             without the key only through the web console of your provider.\n\
             Anything else running on this VPS, like a VPN or a website, will be blocked.\n\
             If you want to keep your own ssh and firewall settings, answer n and run `sai setup --minimal`.",
            ip = config.ip,
        )
    }
}

async fn print_summary(session: &Session, env: &Env<'_>) -> Result<()> {
    let summary = session.output(steps::SUMMARY.script, env).await?;
    let rows: Vec<(&str, &str)> = summary
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    output::info("");
    output::info("Result:");
    for (name, value) in rows {
        output::info(&format!("  {name:<width$}  {value}"));
    }
    output::info("");
    Ok(())
}

fn check_port(config: &Config, minimal: bool) -> Result<()> {
    match (minimal, config.custom_port()) {
        (true, None) => bail!(
            "--minimal keeps your ssh settings, add the ssh port of the VPS to config.toml, for example: port = 22"
        ),
        (false, Some(port)) if port != SSH_PORT => bail!(
            "full setup moves ssh to port {SSH_PORT}, remove `port = {port}` from config.toml or run `sai setup --minimal`"
        ),
        _ => Ok(()),
    }
}

fn script_env(config: &Config) -> Result<Vec<(&'static str, String)>> {
    Ok(vec![
        ("SAI_USER", USER.to_owned()),
        ("SAI_PORT", config.port().to_string()),
        ("SAI_MINIMAL", MINIMAL.to_owned()),
        ("SAI_PUBKEY", config.public_key()?),
        ("SAI_SETUP_MARKER", SETUP_MARKER.to_owned()),
        ("SAI_BUILD_CACHE_MARKER", vps::build_cache_marker()),
        ("SAI_IMAGE", image::base()),
        ("SAI_DOCKERFILE", image::encoded_base()),
    ])
}

async fn run_steps(session: &Session, steps: &[Step], env: &Env<'_>) -> Result<()> {
    for step in steps {
        output::step(step.title);
        session
            .run_script(step.script, env)
            .await
            .with_context(|| format!("step failed: {}", step.title))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_port, plan};
    use crate::config::Config;

    fn config(port: Option<u16>) -> Config {
        let port = port.map_or(String::new(), |port| format!("port = {port}\n"));
        toml::from_str(&format!(
            "ssh_key = \"~/.ssh/id\"\nip = \"1.2.3.4\"\n{port}"
        ))
        .unwrap()
    }

    #[test]
    fn full_setup_owns_the_ssh_port() {
        assert!(check_port(&config(None), false).is_ok());
        assert!(check_port(&config(Some(28522)), false).is_ok());
        assert!(check_port(&config(Some(22)), false).is_err());
    }

    #[test]
    fn minimal_setup_needs_the_existing_port() {
        assert!(check_port(&config(None), true).is_err());
        assert!(check_port(&config(Some(22)), true).is_ok());
        assert_eq!(config(Some(2222)).port(), 2222);
        assert_eq!(config(None).port(), 28522);
    }

    #[test]
    fn explains_what_setup_changes() {
        let full = plan(&config(None), false);
        assert!(full.contains("move ssh from port 22 to 28522"));
        assert!(full.contains("sai setup --minimal"));
        assert!(full.lines().filter(|line| line.starts_with("  - ")).count() >= 7);
        let minimal = plan(&config(Some(2222)), true);
        assert!(minimal.contains("port 2222"));
        assert!(!minimal.contains("ufw firewall"));
    }
}
