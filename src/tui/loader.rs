use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;

use crate::config::Config;
use crate::project::list::{self, Row};
use crate::status::{self, Vps};

pub struct Loaded {
    pub ip: String,
    pub projects: Vec<Row>,
    pub vps: Vps,
}

pub fn spawn() -> Receiver<Result<Loaded, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        sender
            .send(load().map_err(|error| format!("{error:#}")))
            .ok();
    });
    receiver
}

fn load() -> Result<Loaded> {
    Ok(Loaded {
        ip: Config::load()?.ip,
        projects: list::load()?,
        vps: status::load()?,
    })
}
