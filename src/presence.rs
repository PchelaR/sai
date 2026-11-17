use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::thread;

use anyhow::Result;

use crate::{config, logs, mutagen};

pub struct Presence {
    file: Option<File>,
}

pub fn enter(resume: bool) -> Presence {
    let file = open().ok().filter(|file| file.lock_shared().is_ok());
    if resume && file.is_some() {
        thread::spawn(|| {
            if let Err(error) = mutagen::resume_all() {
                logs::error(&error);
            }
        });
    }
    Presence { file }
}

impl Presence {
    pub fn leave(mut self) {
        if self.file.take().is_none() {
            return;
        }
        let Ok(last) = open() else {
            return;
        };
        if last.try_lock().is_ok()
            && let Err(error) = mutagen::pause_all()
        {
            logs::error(&error);
        }
    }
}

fn open() -> Result<File> {
    let path = path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?)
}

fn path() -> Result<PathBuf> {
    Ok(config::dir()?.join("locks").join("running.lock"))
}
