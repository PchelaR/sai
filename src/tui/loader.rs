use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;

use crate::config::Config;
use crate::project::list::{self, Row};
use crate::status::{self, Vps};
use crate::upgrade;

pub struct Loaded {
    pub ip: String,
    pub projects: Vec<Row>,
    pub vps: Vps,
    pub latest: Option<String>,
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
    let ip = Config::load()?.ip;
    let (projects, vps, latest) = thread::scope(|scope| {
        let projects = scope.spawn(list::load);
        let latest = scope.spawn(upgrade::available);
        let vps = status::load();
        (projects.join(), vps, latest.join().ok().flatten())
    });
    let projects = projects.map_err(|_| anyhow::anyhow!("loading the projects crashed"))??;
    Ok(Loaded {
        ip,
        projects,
        vps: vps?,
        latest,
    })
}
