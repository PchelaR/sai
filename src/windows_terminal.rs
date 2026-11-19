//! Windows Terminal answers the agent's bell with a sound and sends Shift+Enter as Enter,
//! unless its settings say otherwise, and nothing on sai's side can change that. With the
//! user's consent sai adds the two settings: it inserts them into the file without
//! rewriting it, so comments and formatting stay, keeps a backup, and writes nothing
//! unless the result still parses

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::{config, output};

const SHOWN: &str = "windows-terminal-tips";
const BACKUP: &str = "settings.json.sai-backup";
const SHIFT_ENTER: &str =
    r#"{ "command": { "action": "sendInput", "input": "\n" }, "keys": "shift+enter" }"#;
const BELL: &str = r#""bellStyle": "taskbar""#;

pub const BELL_TIP: &str = "the agent rings the bell when it finishes or asks, Windows Terminal plays a sound for it. To flash the taskbar instead: Settings > Defaults > Advanced > Bell notification style, uncheck Audible, check Flash taskbar";

pub const SHIFT_ENTER_TIP: &str = "Shift+Enter sends a message instead of starting a new line (Ctrl+J or \\ then Enter work anyway). To make Shift+Enter work: Settings > Open JSON file, add to \"actions\": { \"command\": { \"action\": \"sendInput\", \"input\": \"\\n\" }, \"keys\": \"shift+enter\" }";

pub struct Settings {
    pub bell: bool,
    pub shift_enter: bool,
}

struct File {
    path: PathBuf,
    text: String,
}

fn running_in_it() -> bool {
    cfg!(windows) && env::var_os("WT_SESSION").is_some()
}

fn find() -> Option<File> {
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    [
        "Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
        "Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json",
        "Microsoft/Windows Terminal/settings.json",
    ]
    .iter()
    .map(|relative| local.join(relative))
    .find_map(|path| {
        let text = fs::read_to_string(&path).ok()?;
        Some(File { path, text })
    })
}

/// None outside Windows Terminal or when its settings cannot be read
pub fn settings() -> Option<Settings> {
    if !running_in_it() {
        return None;
    }
    Some(parse(&find()?.text))
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

/// After the first chat in Windows Terminal: offers to set it up, once
pub fn offer_once() {
    if !running_in_it() || !io::stdin().is_terminal() {
        return;
    }
    let Ok(marker) = config::dir().map(|dir| dir.join(SHOWN)) else {
        return;
    };
    if marker.exists() {
        return;
    }
    fs::write(&marker, "").ok();
    let Some(file) = find() else {
        return;
    };
    if tips(&parse(&file.text)).is_empty() {
        return;
    }
    output::info("");
    if let Err(error) = setup(file) {
        output::warning(&format!("{error:#}"));
    }
    // The menu redraws the screen right after a chat, it would wipe this unread
    print!("Press Enter to continue (`sai terminal` does this again later)");
    io::stdout().flush().ok();
    io::stdin().read_line(&mut String::new()).ok();
}

/// `sai terminal`
pub fn run() -> Result<()> {
    if !running_in_it() {
        output::info(
            "Run this in Windows Terminal: it sets up Shift+Enter and the taskbar flash there. Other terminals need no setup",
        );
        return Ok(());
    }
    let file = find().context("cannot find the settings of Windows Terminal")?;
    setup(file)
}

fn setup(file: File) -> Result<()> {
    let settings = parse(&file.text);
    let missing = tips(&settings);
    if missing.is_empty() {
        output::info("Windows Terminal is already set up for sai");
        return Ok(());
    }
    let mut changes = Vec::new();
    if !settings.shift_enter {
        changes.push("Shift+Enter starts a new line in the agent's chat");
    }
    if !settings.bell {
        changes.push("the taskbar flashes instead of a sound when the agent finishes or asks");
    }
    let confirmed = Confirm::new()
        .with_prompt(format!(
            "Change the settings of Windows Terminal so that {}? The current file is kept as {BACKUP}",
            changes.join(", and ")
        ))
        .default(true)
        .interact()?;
    if !confirmed {
        for tip in missing {
            output::info(&format!("Windows Terminal: {tip}"));
        }
        return Ok(());
    }
    let mut text = file.text.clone();
    let mut skipped = Vec::new();
    if !settings.shift_enter {
        text = add_shift_enter(&text).context("cannot find where to add Shift+Enter")?;
    }
    if !settings.bell {
        match add_bell(&text) {
            Some(changed) => text = changed,
            None => skipped.push(BELL_TIP),
        }
    }
    if !is_valid(&file.text) {
        bail!("the settings of Windows Terminal do not parse, nothing was changed");
    }
    if !is_valid(&text) {
        bail!("changing the settings of Windows Terminal would break them, nothing was changed");
    }
    let backup = file.path.with_file_name(BACKUP);
    if !backup.exists() {
        fs::write(&backup, &file.text).context("cannot keep a backup of the settings")?;
    }
    fs::write(&file.path, text).context("cannot write the settings of Windows Terminal")?;
    output::info(
        "Done, Windows Terminal picks the settings up by itself. In a running chat, restart the agent with /exit and `sai chat` for Shift+Enter",
    );
    for tip in skipped {
        output::info(&format!("Windows Terminal: {tip}"));
    }
    Ok(())
}

fn add_shift_enter(text: &str) -> Option<String> {
    if let Some(open) = array_after_key(text, "actions") {
        return Some(insert(text, open, SHIFT_ENTER));
    }
    let root = text.find('{')? + 1;
    let entry = format!("\"actions\": [ {SHIFT_ENTER} ]");
    Some(insert(text, root, &entry))
}

fn add_bell(text: &str) -> Option<String> {
    let profiles = text.find("\"profiles\"")?;
    let defaults = profiles + text[profiles..].find("\"defaults\"")?;
    let open = opening(text, defaults + "\"defaults\"".len(), '{')?;
    Some(insert(text, open, BELL))
}

/// Just after the `[` of `"key": [`
fn array_after_key(text: &str, key: &str) -> Option<usize> {
    let quoted = format!("\"{key}\"");
    text.match_indices(&quoted)
        .find_map(|(start, _)| opening(text, start + quoted.len(), '['))
}

/// Just after `bracket` when `: bracket` follows `from`, comments allowed in between
fn opening(text: &str, from: usize, bracket: char) -> Option<usize> {
    let colon = significant(text, from)?;
    if text[colon..].chars().next()? != ':' {
        return None;
    }
    let open = significant(text, colon + 1)?;
    (text[open..].chars().next()? == bracket).then_some(open + 1)
}

/// Inserts an entry at the start of an object or array, with a comma when more follows
fn insert(text: &str, at: usize, entry: &str) -> String {
    let more = significant(text, at).is_some_and(|next| !text[next..].starts_with([']', '}']));
    let comma = if more { "," } else { "" };
    format!("{}\n        {entry}{comma}{}", &text[..at], &text[at..])
}

/// Position of the next character that is not whitespace or inside a comment
fn significant(text: &str, mut at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    while at < bytes.len() {
        match bytes[at] {
            b' ' | b'\t' | b'\r' | b'\n' => at += 1,
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find('\n').map_or(bytes.len(), |end| at + end);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = text[at + 2..].find("*/").map(|end| at + 2 + end + 2)?;
            }
            _ => return Some(at),
        }
    }
    None
}

/// Parses the way Windows Terminal does: comments and trailing commas allowed
fn is_valid(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(&plain_json(text)).is_ok()
}

fn plain_json(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}');
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for skipped in chars.by_ref() {
                    if last == '*' && skipped == '/' {
                        break;
                    }
                    last = skipped;
                }
                out.push(' ');
            }
            ']' | '}' => {
                let trimmed = out.trim_end().len();
                if out[..trimmed].ends_with(',') {
                    out.truncate(trimmed - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{add_bell, add_shift_enter, is_valid, parse, tips};

    const FRESH: &str = r#"{
    "$schema": "https://aka.ms/terminal-profiles-schema",
    // the user's own comment
    "actions": [
        { "command": { "action": "copy" }, "id": "User.copy" },
    ],
    "profiles": {
        "defaults": {},
        "list": [ { "name": "PowerShell" } ]
    }
}"#;

    #[test]
    fn sees_what_the_settings_already_do() {
        assert_eq!(tips(&parse(FRESH)).len(), 2);
        let audible = parse(r#"{ "profiles": { "defaults": { "bellStyle": "audible" } } }"#);
        assert!(!audible.bell);
        let done = parse(r#"{ "bellStyle": [ "taskbar" ], "keys": "shift+enter" }"#);
        assert!(tips(&done).is_empty());
    }

    #[test]
    fn adds_settings_keeping_the_rest() {
        assert!(is_valid(FRESH));
        let text = add_bell(&add_shift_enter(FRESH).unwrap()).unwrap();
        assert!(is_valid(&text));
        assert!(text.contains("// the user's own comment"));
        assert!(text.contains(r#""input": "\n" }, "keys": "shift+enter" },"#));
        assert!(text.contains(r#""bellStyle": "taskbar""#));
        assert!(tips(&parse(&text)).is_empty());
    }

    #[test]
    fn adds_to_empty_or_missing_places() {
        let empty = r#"{ "actions": [], "profiles": { "defaults": { } } }"#;
        let text = add_bell(&add_shift_enter(empty).unwrap()).unwrap();
        assert!(is_valid(&text));
        assert!(!text.contains(",]") && !text.contains(", ]"));
        let bare = r#"{ /* none */ "profiles": { "list": [] } }"#;
        let text = add_shift_enter(bare).unwrap();
        assert!(is_valid(&text));
        assert!(add_bell(bare).is_none());
    }

    #[test]
    fn refuses_broken_settings() {
        assert!(!is_valid(r#"{ "actions": [ "#));
        assert!(is_valid("\u{feff}{ \"a\": \"x // not a comment\", }"));
    }
}
