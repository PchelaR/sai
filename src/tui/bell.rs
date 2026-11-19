use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crate::logs;
use crate::project::list;

// Windows OpenSSH cannot share one connection, so each check there opens a new one
const POLL: Duration = Duration::from_secs(if cfg!(windows) { 10 } else { 5 });
const BELL: &[u8] = b"\x07";

/// Checks in the background what the agents are doing, so the menu can call the user
/// when an agent finishes a task or asks for permission. Inside a chat the agent's own
/// hooks ring the bell, so the watcher lives only while the menu is on screen
pub struct Watcher {
    pub states: Receiver<HashMap<String, String>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn watch() -> Watcher {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let (sender, states) = mpsc::channel();
    thread::spawn(move || {
        let mut failing = false;
        while !flag.load(Ordering::Relaxed) {
            match list::agent_states() {
                Some(found) => {
                    if failing {
                        logs::line("menu: the VPS answers again, watching the agents");
                        failing = false;
                    }
                    if sender.send(found).is_err() {
                        return;
                    }
                }
                None if !failing => {
                    logs::line("menu: cannot check the agents on the VPS, will keep trying");
                    failing = true;
                }
                None => {}
            }
            thread::sleep(POLL);
        }
    });
    Watcher { states, stop }
}

/// The terminal decides what the bell does: Windows Terminal flashes the taskbar button
/// when the window is not focused, macOS bounces the dock icon
pub fn ring() {
    let mut stdout = io::stdout();
    stdout.write_all(BELL).ok();
    stdout.flush().ok();
}
