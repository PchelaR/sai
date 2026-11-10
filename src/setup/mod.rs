mod credentials;
mod steps;

use anyhow::{Context, Result};

use crate::agent::Agent;
use crate::config::Config;
use crate::output;
use crate::ssh::{self, Env, Session};
use crate::vps::{HOST_ALIAS, SETUP_MARKER, SSH_PORT, USER};
use steps::Step;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    if ssh::is_setup_complete(&config) {
        ssh::install_host_alias(&config)?;
        println!(
            "{} is already configured. Connect with: ssh {HOST_ALIAS}",
            config.ip
        );
        return Ok(());
    }

    let env = script_env(&config)?;
    let credentials = credentials::ask(&config.ip)?;
    let session = Session::connect(&config.ip, &credentials.user, &credentials.password).await?;

    run_steps(&session, steps::PREPARE, &env).await?;
    if let Err(error) = lock_down(&session, &config, &env).await {
        run_steps(&session, steps::ROLLBACK, &env).await?;
        return Err(error.context("ssh lockdown failed, ssh changes were rolled back"));
    }

    ssh::install_host_alias(&config)?;
    println!(
        "Done. {} is configured. Connect with: ssh {HOST_ALIAS}",
        config.ip
    );
    Ok(())
}

async fn lock_down(session: &Session, config: &Config, env: &Env<'_>) -> Result<()> {
    ssh::verify_key_login(config)
        .with_context(|| format!("key login as {USER} on port {SSH_PORT} failed"))?;
    run_steps(session, steps::LOCK_DOWN, env).await?;
    ssh::verify_key_login(config).context("key login failed after lockdown")
}

fn script_env(config: &Config) -> Result<Vec<(&'static str, String)>> {
    let image = Agent::default().image()?;
    Ok(vec![
        ("SAI_USER", USER.to_owned()),
        ("SAI_PORT", SSH_PORT.to_string()),
        ("SAI_PUBKEY", config.public_key()?),
        ("SAI_SETUP_MARKER", SETUP_MARKER.to_owned()),
        ("SAI_IMAGE", image.reference()),
        ("SAI_DOCKERFILE", image.encoded_dockerfile()),
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
