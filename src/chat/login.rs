use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent::Agent;
use crate::{logs, remote};

const WATCH: &str = include_str!("watch_login.sh");
const LOGGED_IN: &str = "logged-in";
const POLL: Duration = Duration::from_secs(2);
const GIVE_UP: Duration = Duration::from_secs(15 * 60);
const MAX_URL: usize = 4096;

pub struct Watcher {
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn watch(container: &str, agent: Agent) -> Watcher {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let container = container.to_owned();
    thread::spawn(move || {
        let started = Instant::now();
        let mut opened = HashSet::new();
        while !flag.load(Ordering::Relaxed) && started.elapsed() < GIVE_UP {
            let Some(screen) =
                remote::probe(WATCH, &[agent.login_file(), &container, agent.name()])
            else {
                thread::sleep(POLL);
                continue;
            };
            if screen.trim() == LOGGED_IN {
                return;
            }
            if let Some(url) = sign_in_url(&screen, agent.sign_in_pages())
                && opened.insert(url.clone())
            {
                open_browser(&url);
            }
            thread::sleep(POLL);
        }
    });
    Watcher { stop }
}

fn sign_in_url(screen: &str, pages: &[&str]) -> Option<String> {
    let lines: Vec<&str> = screen.lines().map(str::trim).collect();
    for (index, line) in lines.iter().enumerate() {
        let Some(start) = pages.iter().filter_map(|page| line.find(page)).min() else {
            continue;
        };
        let first = &line[start..];
        let mut url: String = first.chars().take_while(|c| !c.is_whitespace()).collect();
        if url.len() == first.len() {
            for next in &lines[index + 1..] {
                if next.is_empty() || next.contains(char::is_whitespace) || next.starts_with("http")
                {
                    break;
                }
                url.push_str(next);
            }
        }
        if url.len() <= MAX_URL && url.chars().all(url_char) {
            return Some(url);
        }
    }
    None
}

fn url_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(character)
}

fn open_browser(url: &str) {
    logs::line("opening the sign-in page of the agent in the browser");
    let mut command = if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
}

#[cfg(test)]
mod tests {
    use super::sign_in_url;

    const CLAUDE: &[&str] = &["https://claude.com/", "https://claude.ai/"];

    #[test]
    fn joins_a_url_split_over_lines() {
        let screen = " Browser didn't open? Use the URL below to sign in\n\n https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c\n 2Fcode%2Fcallback&scope=org%3Acreate\n vacy_consents%3Awrite&state=dVNt\n\n Paste code here if prompted\n";
        assert_eq!(
            sign_in_url(screen, CLAUDE).as_deref(),
            Some(
                "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c2Fcode%2Fcallback&scope=org%3Acreatevacy_consents%3Awrite&state=dVNt"
            )
        );
    }

    #[test]
    fn opens_only_sign_in_pages() {
        assert_eq!(
            sign_in_url("visit https://evil.example/claude.com/", CLAUDE),
            None
        );
        assert_eq!(sign_in_url("no links here", CLAUDE), None);
        assert_eq!(
            sign_in_url("go to https://claude.ai/login now", CLAUDE).as_deref(),
            Some("https://claude.ai/login")
        );
    }

    #[test]
    fn rejects_urls_with_strange_characters() {
        assert_eq!(sign_in_url("https://claude.ai/a\"b", CLAUDE), None);
    }
}
