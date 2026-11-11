use anyhow::Result;
use dialoguer::{Input, Password};

use crate::ssh::Auth;

pub struct Credentials {
    pub user: String,
    pub auth: Auth,
}

pub fn ask(ip: &str) -> Result<Credentials> {
    let user: String = Input::new()
        .with_prompt(format!("Initial user on {ip}"))
        .default("root".to_owned())
        .interact_text()?;
    let password = Password::new()
        .with_prompt(format!(
            "Password for {user}@{ip} (leave empty to log in with your ssh key)"
        ))
        .allow_empty_password(true)
        .interact()?;
    let auth = if password.is_empty() {
        Auth::Key
    } else {
        Auth::Password(password)
    };
    Ok(Credentials { user, auth })
}
