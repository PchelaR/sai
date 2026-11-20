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

pub const SHIFT_ENTER_TIP: &str = "Shift+Enter sends a message instead of starting a new line (Ctrl+Enter and Ctrl+J start one anyway). To make Shift+Enter work: Settings > Open JSON file, add to \"actions\": { \"command\": { \"action\": \"sendInput\", \"input\": \"\\n\" }, \"keys\": \"shift+enter\" }";

pub const SHIFT_ENTER_TAKEN_TIP: &str = "Shift+Enter is bound to another action in the settings of Windows Terminal, so it cannot start a new line in the agent's chat. Ctrl+Enter and Ctrl+J start one anyway. To use Shift+Enter: Settings > Actions, remove what Shift+Enter does, then run `sai terminal`";

pub struct Settings {
    pub bell: bool,
    shift_enter: Binding,
}

/// What Shift+Enter does in the settings
#[derive(Clone, Copy, Debug, PartialEq)]
enum Binding {
    Newline,
    Missing,
    /// Something else, or both a new line and something else: sai leaves that alone
    Other,
}

struct File {
    path: PathBuf,
    text: String,
}

fn running_in_it() -> bool {
    cfg!(windows) && env::var_os("WT_SESSION").is_some()
}

/// Every installed Windows Terminal: stable, Preview and unpackaged ones keep separate
/// settings, and nothing says which of them runs sai
fn find() -> Vec<File> {
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
    .filter_map(|path| {
        let text = fs::read_to_string(&path).ok()?;
        Some(File { path, text })
    })
    .collect()
}

/// None outside Windows Terminal or when its settings cannot be read. With several
/// installed, what any of them still lacks
pub fn settings() -> Option<Settings> {
    if !running_in_it() {
        return None;
    }
    find().iter().map(|file| parse(&file.text)).reduce(combine)
}

/// What several installed terminals still lack together
fn combine(all: Settings, one: Settings) -> Settings {
    Settings {
        bell: all.bell && one.bell,
        shift_enter: match (all.shift_enter, one.shift_enter) {
            (Binding::Other, _) | (_, Binding::Other) => Binding::Other,
            (Binding::Missing, _) | (_, Binding::Missing) => Binding::Missing,
            _ => Binding::Newline,
        },
    }
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
        shift_enter: shift_enter(text),
    }
}

/// Reads the bindings for real: a rough look took `ctrl+shift+enter` or Shift+Enter bound
/// to another action for the new line. Both the old format (keys next to the command) and
/// the one since 1.21 (keybindings pointing at an action id) count
fn shift_enter(text: &str) -> Binding {
    // Broken settings are refused when sai would write them
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&plain_json(text)) else {
        return Binding::Missing;
    };
    let entries: Vec<&serde_json::Value> = ["actions", "keybindings"]
        .iter()
        .filter_map(|key| json.get(key)?.as_array())
        .flatten()
        .collect();
    let command_of = |id: &str| {
        entries
            .iter()
            .find(|entry| entry.get("id").and_then(|v| v.as_str()) == Some(id))
            .and_then(|entry| entry.get("command"))
    };
    let mut found = Vec::new();
    for entry in &entries {
        let Some(keys) = entry.get("keys") else {
            continue;
        };
        let keys: Vec<&str> = match keys {
            serde_json::Value::String(one) => vec![one.as_str()],
            serde_json::Value::Array(many) => many.iter().filter_map(|v| v.as_str()).collect(),
            _ => Vec::new(),
        };
        if !keys.iter().any(|keys| is_shift_enter(keys)) {
            continue;
        }
        let command = match entry.get("command") {
            Some(command) => Some(command),
            None => entry
                .get("id")
                .and_then(|id| id.as_str())
                .and_then(command_of),
        };
        let newline = command.is_some_and(|command| {
            command.get("action").and_then(|v| v.as_str()) == Some("sendInput")
                && command.get("input").and_then(|v| v.as_str()) == Some("\n")
        });
        found.push(newline);
    }
    match found.as_slice() {
        [] => Binding::Missing,
        all if all.iter().all(|newline| *newline) => Binding::Newline,
        _ => Binding::Other,
    }
}

fn is_shift_enter(keys: &str) -> bool {
    let mut parts: Vec<String> = keys
        .split('+')
        .map(|part| part.trim().to_lowercase())
        .collect();
    parts.sort();
    parts == ["enter", "shift"]
}

pub fn tips(settings: &Settings) -> Vec<&'static str> {
    let mut tips = Vec::new();
    if !settings.bell {
        tips.push(BELL_TIP);
    }
    match settings.shift_enter {
        Binding::Newline => {}
        Binding::Missing => tips.push(SHIFT_ENTER_TIP),
        Binding::Other => tips.push(SHIFT_ENTER_TAKEN_TIP),
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
    let files = find();
    if files.iter().all(|file| tips(&parse(&file.text)).is_empty()) {
        return;
    }
    output::info("");
    if let Err(error) = setup(files) {
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
            "Run this in Windows Terminal: it sets up Shift+Enter and the taskbar flash there",
        );
        return Ok(());
    }
    let files = find();
    if files.is_empty() {
        bail!("cannot find the settings of Windows Terminal");
    }
    setup(files)
}

fn setup(files: Vec<File>) -> Result<()> {
    let all: Vec<(File, Settings)> = files
        .into_iter()
        .map(|file| {
            let settings = parse(&file.text);
            (file, settings)
        })
        .collect();
    let taken = all
        .iter()
        .any(|(_, settings)| settings.shift_enter == Binding::Other);
    if taken {
        output::warning(SHIFT_ENTER_TAKEN_TIP);
    }
    let add_shift_enter_to = |settings: &Settings| settings.shift_enter == Binding::Missing;
    let shift_enter = all.iter().any(|(_, settings)| add_shift_enter_to(settings));
    let bell = all.iter().any(|(_, settings)| !settings.bell);
    if !shift_enter && !bell {
        if !taken {
            output::info("Windows Terminal is already set up for sai");
        }
        return Ok(());
    }
    let mut changes = Vec::new();
    if shift_enter {
        changes.push("Shift+Enter starts a new line in the agent's chat");
    }
    if bell {
        changes.push("the taskbar flashes instead of a sound when the agent finishes or asks");
    }
    // A prompt wider than the window comes out twice: dialoguer redraws it line by line
    output::info(&format!(
        "sai can change the settings of Windows Terminal so that {}. The current file is kept as {BACKUP}",
        changes.join(", and ")
    ));
    let confirmed = Confirm::new()
        .with_prompt("Change them?")
        .default(true)
        .interact()?;
    if !confirmed {
        if shift_enter {
            output::info(&format!("Windows Terminal: {SHIFT_ENTER_TIP}"));
        }
        if bell {
            output::info(&format!("Windows Terminal: {BELL_TIP}"));
        }
        return Ok(());
    }
    let mut skipped = false;
    for (file, settings) in &all {
        let mut text = file.text.clone();
        if add_shift_enter_to(settings) {
            text = add_shift_enter(&text).context("cannot find where to add Shift+Enter")?;
        }
        if !settings.bell {
            match add_bell(&text) {
                Some(changed) => text = changed,
                None => skipped = true,
            }
        }
        if text == file.text {
            continue;
        }
        if !is_valid(&file.text) {
            bail!(
                "the settings of Windows Terminal in {} do not parse, nothing was changed",
                file.path.display()
            );
        }
        if !is_valid(&text) {
            bail!(
                "changing the settings of Windows Terminal in {} would break them, nothing was changed",
                file.path.display()
            );
        }
        let backup = file.path.with_file_name(BACKUP);
        if !backup.exists() {
            fs::write(&backup, &file.text).context("cannot keep a backup of the settings")?;
        }
        fs::write(&file.path, text).context("cannot write the settings of Windows Terminal")?;
    }
    output::info("Done, Windows Terminal picks the settings up by itself, also in a running chat");
    if skipped {
        output::info(&format!("Windows Terminal: {BELL_TIP}"));
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
    use super::{Binding, Settings, add_bell, add_shift_enter, combine, is_valid, parse, tips};

    #[test]
    fn several_terminals_need_what_any_of_them_lacks() {
        let settings = |bell, shift_enter| Settings { bell, shift_enter };
        let done = || settings(true, Binding::Newline);
        assert!(tips(&combine(done(), done())).is_empty());
        let both = combine(done(), settings(false, Binding::Missing));
        assert!(!both.bell);
        assert_eq!(both.shift_enter, Binding::Missing);
        let taken = combine(
            settings(true, Binding::Other),
            settings(true, Binding::Missing),
        );
        assert_eq!(taken.shift_enter, Binding::Other);
    }

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
        let done = parse(
            r#"{ "bellStyle": [ "taskbar" ], "actions": [ { "command": { "action": "sendInput", "input": "\n" }, "keys": "Shift+Enter" } ] }"#,
        );
        assert!(tips(&done).is_empty());
    }

    #[test]
    fn reads_what_shift_enter_really_does() {
        let shift_enter = |text: &str| parse(text).shift_enter;
        // Since 1.21 Windows Terminal moves keys out of actions on its own
        let split = r#"{
            "actions": [ { "command": { "action": "sendInput", "input": "\u000a" }, "id": "User.sendInput.1" } ],
            "keybindings": [ { "id": "User.sendInput.1", "keys": "enter+shift" } ],
        }"#;
        assert_eq!(shift_enter(split), Binding::Newline);
        let other_keys = r#"{ "actions": [ { "command": { "action": "sendInput", "input": "\n" }, "keys": "ctrl+shift+enter" } ] }"#;
        assert_eq!(shift_enter(other_keys), Binding::Missing);
        let other_action =
            r#"{ "actions": [ { "command": "toggleFullscreen", "keys": "shift+enter" } ] }"#;
        assert_eq!(shift_enter(other_action), Binding::Other);
        let unbound = r#"{ "keybindings": [ { "id": null, "keys": "shift+enter" } ] }"#;
        assert_eq!(shift_enter(unbound), Binding::Other);
        let both = r#"{ "actions": [
            { "command": { "action": "sendInput", "input": "\n" }, "keys": "shift+enter" },
            { "command": "find", "keys": ["shift+enter"] } ] }"#;
        assert_eq!(shift_enter(both), Binding::Other);
        assert_eq!(shift_enter(r#"{ "actions": [] }"#), Binding::Missing);
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
