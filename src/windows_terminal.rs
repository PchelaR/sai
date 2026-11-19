//! Windows Terminal answers the agent's bell with a sound and sends Shift+Enter as Enter,
//! unless its settings say otherwise. sai cannot fix either from its side, so it looks
//! at the settings and says what to add. It never edits them: the file allows comments
//! and the user's own formatting, which a rewrite would lose

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use crate::{config, output};

const SHOWN: &str = "windows-terminal-tips";

pub const BELL_TIP: &str = "the agent rings the bell when it finishes or asks, Windows Terminal plays a sound for it. To flash the taskbar instead: Settings > Defaults > Advanced > Bell notification style, uncheck Audible, check Flash taskbar";

pub const SHIFT_ENTER_TIP: &str = "Shift+Enter sends a message instead of starting a new line (Ctrl+J or \\ then Enter work anyway). To make Shift+Enter work: Settings > Open JSON file, add to \"actions\": { \"command\": { \"action\": \"sendInput\", \"input\": \"\\n\" }, \"keys\": \"shift+enter\" }";

pub struct Settings {
    pub bell: bool,
    pub shift_enter: bool,
}

/// None outside Windows Terminal or when its settings cannot be read
pub fn settings() -> Option<Settings> {
    if !cfg!(windows) || env::var_os("WT_SESSION").is_none() {
        return None;
    }
    let text = paths()
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())?;
    Some(parse(&text))
}

fn paths() -> Vec<PathBuf> {
    let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        "Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
        "Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json",
        "Microsoft/Windows Terminal/settings.json",
    ]
    .iter()
    .map(|relative| local.join(relative))
    .collect()
}

/// A rough look rather than a JSON parse: the file has comments and trailing commas
fn parse(text: &str) -> Settings {
    let lower = text.to_lowercase();
    let bell = lower.match_indices("\"bellstyle\"").any(|(start, _)| {
        let value: String = lower[start..].chars().take(80).collect();
        ["taskbar", "window", "\"all\""]
            .iter()
            .any(|style| value.contains(style))
    });
    Settings {
        bell,
        shift_enter: lower.contains("shift+enter"),
    }
}

pub fn tips(settings: &Settings) -> Vec<&'static str> {
    let mut tips = Vec::new();
    if !settings.bell {
        tips.push(BELL_TIP);
    }
    if !settings.shift_enter {
        tips.push(SHIFT_ENTER_TIP);
    }
    tips
}

/// Before the first chat in Windows Terminal, once. `sai doctor` repeats them
pub fn hint_once() {
    let Some(settings) = settings() else {
        return;
    };
    let Ok(marker) = config::dir().map(|dir| dir.join(SHOWN)) else {
        return;
    };
    if marker.exists() {
        return;
    }
    let tips = tips(&settings);
    fs::write(marker, "").ok();
    if tips.is_empty() {
        return;
    }
    output::info("");
    for tip in tips {
        output::info(&format!("Windows Terminal: {tip}"));
    }
    // The menu redraws the screen right after a chat, it would wipe the tips unread
    if io::stdin().is_terminal() {
        print!("Press Enter to continue (shown once, `sai doctor` repeats it)");
        io::stdout().flush().ok();
        io::stdin().read_line(&mut String::new()).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, tips};

    #[test]
    fn sees_what_the_settings_already_do() {
        let fresh = parse(r#"{ "profiles": { "defaults": {} }, "actions": [] }"#);
        assert_eq!(tips(&fresh).len(), 2);
        let done = parse(
            r#"{
                // mine
                "profiles": { "defaults": { "bellStyle": [ "taskbar" ] } },
                "actions": [ { "command": { "action": "sendInput", "input": "\n" }, "keys": "shift+enter" }, ]
            }"#,
        );
        assert!(tips(&done).is_empty());
        let audible = parse(r#"{ "profiles": { "defaults": { "bellStyle": "audible" } } }"#);
        assert!(!audible.bell);
    }
}
