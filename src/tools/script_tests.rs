//! The report of what an answer changed and the snapshots, run for real on this machine.
//! Linux only: the scripts target the VPS and use GNU tools
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use super::{SNAPSHOTS, TURN_REPORT, snapshot_excludes, watched_files};
use crate::testing::{TempDir, has};

fn turn(dir: &TempDir, mode: &str) -> String {
    let mut child = Command::new("bash")
        .arg(dir.join("turn-report"))
        .arg(mode)
        .env("SAI_TURN_WORKSPACE", dir.join("ws"))
        .env("SAI_TURN_STATE", dir.join("state"))
        .env("SAI_TURN_EXCLUDES", dir.join("excludes"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The hook input, as the agent sends it
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "a hook must never fail");
    String::from_utf8(output.stdout).unwrap()
}

fn message(json: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(json).expect("the hook prints JSON");
    value["systemMessage"].as_str().unwrap().to_owned()
}

fn write(dir: &TempDir, path: &str, content: &[u8]) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn reports_what_an_answer_changed() {
    if !has("rsync") || !has("git") {
        return;
    }
    let dir = TempDir::new("turn");
    fs::write(dir.join("turn-report"), TURN_REPORT).unwrap();
    fs::write(dir.join("excludes"), snapshot_excludes()).unwrap();
    write(&dir, "ws/front/src/keep.rs", b"a\nb\nc\n");
    write(&dir, "ws/front/gone.txt", b"old\n");
    write(&dir, "ws/front/touched.txt", b"same\n");
    write(&dir, "ws/front/node_modules/x/i.js", b"x\n");
    assert_eq!(
        turn(&dir, "report"),
        "",
        "no report before the first message"
    );
    assert_eq!(
        turn(&dir, "start"),
        "",
        "start adds nothing to the agent's context"
    );

    write(&dir, "ws/front/src/keep.rs", b"a\nB\nc\nd\n");
    fs::remove_file(dir.join("ws/front/gone.txt")).unwrap();
    write(&dir, "ws/front/src/new \"file\".rs", b"n1\nn2\n");
    write(&dir, "ws/front/real.bin", b"\x00\x01binary");
    Command::new("touch")
        .arg(dir.join("ws/front/touched.txt"))
        .status()
        .unwrap();
    write(&dir, "ws/front/node_modules/x/i.js", b"x\ny\n");
    write(&dir, "ws/front/.env", b"SECRET=1\n");

    let text = message(&turn(&dir, "report"));
    assert!(
        text.starts_with("Changed in this answer: 4 files, +4 -2"),
        "{text}"
    );
    assert!(text.contains("front/src/keep.rs  +2 -1"), "{text}");
    assert!(text.contains("front/gone.txt  deleted, -1"), "{text}");
    assert!(
        text.contains("front/src/new \"file\".rs  new, +2"),
        "{text}"
    );
    assert!(text.contains("front/real.bin  new, binary"), "{text}");
    for hidden in ["touched.txt", "node_modules", ".env"] {
        assert!(!text.contains(hidden), "{hidden} in {text}");
    }

    turn(&dir, "start");
    assert_eq!(turn(&dir, "report"), "", "nothing changed in this answer");
}

#[test]
fn shortens_long_reports() {
    if !has("rsync") || !has("git") {
        return;
    }
    let dir = TempDir::new("turn-long");
    fs::write(dir.join("turn-report"), TURN_REPORT).unwrap();
    fs::write(dir.join("excludes"), "").unwrap();
    fs::create_dir_all(dir.join("ws")).unwrap();
    turn(&dir, "start");
    for index in 0..40 {
        write(&dir, &format!("ws/f{index:02}.txt"), b"x\n");
    }
    let text = message(&turn(&dir, "report"));
    assert!(text.starts_with("Changed in this answer: 40 files, +40 -0"));
    assert!(text.ends_with("and 10 more"), "{text}");
    assert_eq!(text.lines().count(), 32);
}

struct Snapshots {
    home: TempDir,
}

impl Snapshots {
    fn new() -> Self {
        let home = TempDir::new("snapshots");
        let tools = home.join(".sai");
        fs::create_dir_all(tools.join("bin")).unwrap();
        let script = tools.join("bin/snapshots");
        fs::write(&script, SNAPSHOTS).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(tools.join("snapshot-excludes"), snapshot_excludes()).unwrap();
        fs::write(tools.join("watched-files"), watched_files()).unwrap();
        Self { home }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(self.home.join(".sai/bin/snapshots"))
            .args(args)
            .env("HOME", &self.home.path)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "snapshots {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn file(&self, path: &str) -> String {
        fs::read_to_string(self.home.join(&format!("projects/shop/{path}"))).unwrap()
    }

    fn write(&self, path: &str, content: &str) {
        write(
            &self.home,
            &format!("projects/shop/{path}"),
            content.as_bytes(),
        );
    }

    /// Snapshots are named by the second
    fn next_second() {
        thread::sleep(Duration::from_millis(1100));
    }
}

#[test]
fn snapshots_keep_changes_and_restore_them() {
    if !has("rsync") || !has("flock") {
        return;
    }
    let snapshots = Snapshots::new();
    snapshots.write("front/app.py", "print(1)\n");
    snapshots.write("front/node_modules/x.js", "dependency\n");
    snapshots.write("front/.env", "SECRET=1\n");
    snapshots.ok(&["take", "shop"]);
    let first = snapshots.ok(&["list", "shop"]);
    assert_eq!(first.lines().count(), 1);
    assert!(first.ends_with("\tfirst snapshot\n"), "{first}");
    let first_stamp = first.split('\t').next().unwrap().to_owned();

    Snapshots::next_second();
    snapshots.ok(&["take", "shop"]);
    assert_eq!(
        snapshots.ok(&["list", "shop"]).lines().count(),
        1,
        "nothing changed, no new snapshot"
    );

    snapshots.write("front/app.py", "print(2)\n");
    snapshots.write("front/Makefile", "all:\n");
    snapshots.ok(&["take", "shop"]);
    let list = snapshots.ok(&["list", "shop"]);
    assert_eq!(list.lines().count(), 2, "{list}");
    assert!(
        list.lines().next().unwrap().ends_with("changed 1, added 1"),
        "{list}"
    );
    let watched = snapshots.ok(&["watched", "shop"]);
    assert!(watched.contains("front/Makefile"), "{watched}");
    snapshots.ok(&["reviewed", "shop"]);
    assert_eq!(snapshots.ok(&["watched", "shop"]), "");

    let stored = snapshots
        .home
        .join(&format!("snapshots/shop/{first_stamp}"));
    assert!(!stored.join("front/node_modules").exists());
    assert!(!stored.join("front/.env").exists(), "secrets stay out");

    // The user changed the file after the last snapshot: the restore keeps that first
    Snapshots::next_second();
    snapshots.write("front/app.py", "print(3)\n");
    snapshots.ok(&["restore", "shop", &first_stamp, "front/app.py", "front"]);
    assert_eq!(snapshots.file("front/app.py"), "print(1)\n");
    assert_eq!(
        snapshots.file("front/Makefile"),
        "all:\n",
        "one file restored"
    );
    assert_eq!(snapshots.file("front/.env"), "SECRET=1\n");
    assert_eq!(
        snapshots.ok(&["list", "shop"]).lines().count(),
        3,
        "the state before the restore is kept"
    );

    snapshots.ok(&["restore", "shop", &first_stamp, "", "front"]);
    assert!(!snapshots.home.join("projects/shop/front/Makefile").exists());
    assert_eq!(
        snapshots.file("front/node_modules/x.js"),
        "dependency\n",
        "a whole restore leaves dependencies alone"
    );

    let missing = snapshots.run(&["restore", "shop", "20200101T000000Z", "", "front"]);
    assert!(!missing.status.success());
    snapshots.ok(&["purge", "shop"]);
    assert!(snapshots.ok(&["list", "shop"]).is_empty());
}
