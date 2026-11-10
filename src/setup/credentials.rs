use anyhow::Result;
use dialoguer::{Input, Password};

pub struct Credentials {
    pub user: String,
    pub password: String,
}

pub fn ask(ip: &str) -> Result<Credentials> {
    let user: String = Input::new()
        .with_prompt(format!("Initial user on {ip}"))
        .default("root".to_owned())
        .interact_text()?;
    let password = Password::new()
        .with_prompt(format!("Password for {user}@{ip}"))
        .interact()?;
    Ok(Credentials { user, password })
}
