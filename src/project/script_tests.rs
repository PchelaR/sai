//! Scripts that delete or move projects on the VPS, run for real: they must refuse
//! anything outside the project. Docker is stood in for by a shell function, so these run
//! everywhere on Linux. Linux only
use std::fs;
use std::path::Path;

use super::files::REMOVE_SKIPPED;
use crate::testing::{self, TempDir, fails, ok};

#[test]
fn removes_skipped_items_only_inside_the_project() {
    let home = TempDir::new("skipped");
    let root = home.join("projects/shop/front");
    for file in ["data/raw/a.csv", "keep.txt", "logs/x.log"] {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }
    let root_text = root.to_str().unwrap();
    ok(testing::run(
        REMOVE_SKIPPED,
        &[root_text, "data/raw", "logs/x.log"],
        &home.path,
    ));
    assert!(!root.join("data/raw").exists());
    assert!(root.join("data").exists(), "only what was named");
    assert!(!root.join("logs/x.log").exists());
    assert!(root.join("keep.txt").exists());

    for bad in ["../front", "a/../../x", "./keep.txt", "/etc"] {
        fails(testing::run(REMOVE_SKIPPED, &[root_text, bad], &home.path));
    }
    let outside = home.join("elsewhere");
    fs::create_dir_all(outside.join("x")).unwrap();
    fails(testing::run(
        REMOVE_SKIPPED,
        &[outside.to_str().unwrap(), "x"],
        &home.path,
    ));
    assert!(outside.join("x").exists());
    assert!(root.join("keep.txt").exists());
}

const DETACH: &str = include_str!("scripts/detach.sh");
const REMOVE: &str = include_str!("scripts/remove.sh");
const RENAME: &str = include_str!("scripts/rename.sh");
const RELEASE_AGENT: &str = include_str!("scripts/release_agent.sh");
const MIGRATE: &str = include_str!("scripts/migrate.sh");

/// Docker as the scripts see it: a chat is open while $HOME/chat-open exists, the agent
/// container exists with the label in $HOME/label, `docker rm` leaves $HOME/removed-<name>
const DOCKER: &str = r#"
docker() {
  case "$1" in
    exec) [ -e "$HOME/chat-open" ] ;;
    container) [ -e "$HOME/label" ] && cat "$HOME/label" ;;
    rm) touch "$HOME/removed-$3" ;;
    *) ;;
  esac
}
"#;

fn with_docker(script: &str) -> String {
    format!("{DOCKER}{script}")
}

fn files(root: &Path, paths: &[&str]) {
    for path in paths {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }
}

fn text(path: impl AsRef<Path>) -> String {
    path.as_ref().to_str().unwrap().to_owned()
}

#[test]
fn detaching_removes_one_folder_of_a_project_only() {
    let home = TempDir::new("detach");
    files(
        &home.path,
        &[
            "projects/shop/front/a.rs",
            "projects/shop/back/b.rs",
            "secret.txt",
        ],
    );
    let front = text(home.join("projects/shop/front"));
    ok(testing::run(DETACH, &[&front], &home.path));
    assert!(!home.join("projects/shop/front").exists());
    assert!(home.join("projects/shop/back/b.rs").exists());

    let root = text(&home.path);
    for bad in [
        format!("{root}/projects/shop"),
        format!("{root}/projects/shop/../../secret.txt"),
        format!("{root}/projects/shop/./back"),
        format!("{root}/projects/shop//"),
        format!("{root}/secret.txt"),
        "/etc/x".to_owned(),
    ] {
        fails(testing::run(DETACH, &[&bad], &home.path));
    }
    assert!(home.join("secret.txt").exists());
    assert!(home.join("projects/shop/back/b.rs").exists());
}

#[test]
fn removing_a_project_touches_nothing_of_the_others() {
    let home = TempDir::new("remove");
    files(
        &home.path,
        &[
            "projects/shop/front/a.rs",
            "agents/shop/claude/settings.json",
            "projects/shop2/a.rs",
            "agents/shopper/claude/settings.json",
        ],
    );
    let remove = |container: &str, workspace: &str, mode: &str| {
        testing::run(
            &with_docker(REMOVE),
            &[container, &text(home.join(workspace)), mode],
            &home.path,
        )
    };
    // Without --purge the files stay
    assert_eq!(
        ok(remove("sai-shop", "projects/shop", "keep")).trim(),
        "found"
    );
    assert!(home.join("projects/shop/front/a.rs").exists());
    fs::write(home.join("label"), "claude").unwrap();
    ok(remove("sai-shop", "projects/shop", "keep"));
    assert!(home.join("removed-sai-shop").exists(), "the container goes");

    ok(remove("sai-shop", "projects/shop", "purge"));
    assert!(!home.join("projects/shop").exists());
    assert!(!home.join("agents/shop").exists());
    assert!(home.join("projects/shop2/a.rs").exists());
    assert!(home.join("agents/shopper/claude/settings.json").exists());

    fs::remove_file(home.join("label")).unwrap();
    assert_eq!(
        ok(remove("sai-gone", "projects/gone", "purge")).trim(),
        "",
        "nothing found"
    );

    for (container, workspace) in [
        ("sai-shop2", "projects/shopper"),
        ("sai-..", "projects/.."),
        ("sai-x", "projects/shop2/x"),
        ("sai-", "projects/"),
        ("sai-agents", "agents"),
    ] {
        fails(remove(container, workspace, "purge"));
    }
    fails(testing::run(
        &with_docker(REMOVE),
        &["sai-etc", "/etc", "purge"],
        &home.path,
    ));
    assert!(home.join("projects/shop2/a.rs").exists());
    assert!(home.join("agents/shopper").exists());
}

#[test]
fn renaming_moves_the_project_once_its_agent_is_idle() {
    let home = TempDir::new("rename");
    files(
        &home.path,
        &[
            "projects/shop/front/a.rs",
            "snapshots/shop/2026-01-01/front/a.rs",
            "agents/shop/claude/settings.json",
            "projects/other/a.rs",
        ],
    );
    let rename = |mode: &str, old: &str, new: &str| {
        testing::run(&with_docker(RENAME), &[mode, old, new], &home.path)
    };
    ok(rename("check", "shop", "store"));
    let taken = fails(rename("check", "shop", "other"));
    assert!(taken.contains("sai rm other --purge"), "{taken}");
    fs::write(home.join("chat-open"), "").unwrap();
    let busy = fails(rename("check", "shop", "store"));
    assert!(busy.contains("/exit"), "{busy}");
    fs::remove_file(home.join("chat-open")).unwrap();

    ok(rename("move", "shop", "store"));
    assert!(
        home.join("removed-sai-shop").exists(),
        "the old container goes"
    );
    for moved in [
        "projects/store/front/a.rs",
        "snapshots/store/2026-01-01/front/a.rs",
        "agents/store/claude/settings.json",
    ] {
        assert!(home.join(moved).exists(), "{moved}");
    }
    for root in ["projects", "snapshots", "agents"] {
        assert!(!home.join(&format!("{root}/shop")).exists());
    }
    assert!(home.join("projects/other/a.rs").exists());
}

#[test]
fn switching_agents_waits_for_the_chat_and_replaces_the_container() {
    let home = TempDir::new("release");
    let release = |agent: &str| {
        testing::run(
            &with_docker(RELEASE_AGENT),
            &["sai-shop", agent],
            &home.path,
        )
    };
    // No container yet, or the same agent: nothing to do
    ok(release("codex"));
    fs::write(home.join("label"), "claude").unwrap();
    ok(release("claude"));
    assert!(!home.join("removed-sai-shop").exists());

    fs::write(home.join("chat-open"), "").unwrap();
    let busy = fails(release("codex"));
    assert!(busy.contains("claude is still running in shop"), "{busy}");
    assert!(!home.join("removed-sai-shop").exists());
    fs::remove_file(home.join("chat-open")).unwrap();
    ok(release("codex"));
    assert!(home.join("removed-sai-shop").exists());
}

#[test]
fn old_single_folder_projects_get_their_folder() {
    let home = TempDir::new("migrate");
    files(
        &home.path,
        &[
            "projects/shop/src/main.rs",
            "projects/shop/Cargo.toml",
            "snapshots/shop/2026-01-01/src/main.rs",
            "snapshots/shop/2026-01-02/front/src/main.rs",
        ],
    );
    let migrate = |mode: &str| {
        testing::run(
            &with_docker(MIGRATE),
            &[
                mode,
                "sai-shop",
                &text(home.join("projects/shop")),
                &text(home.join("snapshots/shop")),
                "front",
            ],
            &home.path,
        )
    };
    fs::write(home.join("label"), "claude").unwrap();
    assert_eq!(ok(migrate("check")).trim(), "claude", "the agent it ran");
    ok(migrate("move"));
    assert!(home.join("projects/shop/front/src/main.rs").exists());
    assert!(home.join("projects/shop/front/Cargo.toml").exists());
    assert!(!home.join("projects/shop/src").exists());
    assert!(
        home.join("snapshots/shop/2026-01-01/front/src/main.rs")
            .exists()
    );
    assert!(
        home.join("snapshots/shop/2026-01-02/front/src/main.rs")
            .exists()
            && !home.join("snapshots/shop/2026-01-02/front/front").exists(),
        "already nested: left alone"
    );
    // Run twice, as after an interrupted upgrade: nothing moves again
    ok(migrate("move"));
    assert!(home.join("projects/shop/front/src/main.rs").exists());

    fs::write(home.join("chat-open"), "").unwrap();
    fails(migrate("check"));
}

#[test]
fn renaming_and_migrating_refuse_paths_outside_the_projects() {
    let home = TempDir::new("guards");
    files(&home.path, &["projects/shop/a.rs", "keep/b.rs"]);
    for (old, new) in [
        ("shop", "../keep"),
        ("..", "x"),
        ("shop", ""),
        ("Shop", "store"),
    ] {
        fails(testing::run(
            &with_docker(RENAME),
            &["move", old, new],
            &home.path,
        ));
    }
    let root = text(&home.path);
    for (workspace, folder) in [
        (format!("{root}/keep"), "front"),
        (format!("{root}/projects/shop/a"), "front"),
        (format!("{root}/projects/.."), "front"),
        (format!("{root}/projects/shop"), ".."),
        (format!("{root}/projects/shop"), "a/b"),
    ] {
        fails(testing::run(
            &with_docker(MIGRATE),
            &[
                "move",
                "sai-shop",
                &workspace,
                &format!("{root}/snapshots/shop"),
                folder,
            ],
            &home.path,
        ));
    }
    assert!(home.join("keep/b.rs").exists());
    assert!(home.join("projects/shop/a.rs").exists(), "nothing moved");
}
