//! Scripts of `sai chat` run for real. Linux only, Docker ones with SAI_DOCKER_TESTS
use std::fs;
use std::os::unix::fs::MetadataExt;

use base64::{Engine, prelude::BASE64_STANDARD};

use super::{OLD_CHATS, WRITE_INSTRUCTIONS};
use crate::testing::{self, TempDir, fails, ok, shell, unique};

#[test]
fn instructions_are_written_in_place_inside_the_agents_folder() {
    let home = TempDir::new("instructions");
    let file = home.join("agents/shop/instructions.md");
    let path = file.to_str().unwrap();
    let write = |text: &str| {
        testing::run(
            WRITE_INSTRUCTIONS,
            &[path, &BASE64_STANDARD.encode(text)],
            &home.path,
        )
    };
    ok(write("rules"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "rules");
    let inode = fs::metadata(&file).unwrap().ino();
    ok(write("new rules\n## Services"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "new rules\n## Services");
    assert_eq!(
        fs::metadata(&file).unwrap().ino(),
        inode,
        "the running container sees the file through a mount of its inode"
    );
    let outside = home.join("projects/shop/instructions.md");
    fails(testing::run(
        WRITE_INSTRUCTIONS,
        &[outside.to_str().unwrap(), &BASE64_STANDARD.encode("x")],
        &home.path,
    ));
    assert!(!outside.exists());
}

#[test]
fn old_chats_move_into_a_project_and_the_volume_goes() {
    if !testing::docker() {
        return;
    }
    let home = TempDir::new("old-chats");
    let agent = unique("oldchats");
    let volume = format!("sai-state-{agent}");
    let image = "debian:trixie-slim";
    shell(&format!(
        "docker run --rm -v {volume}:/old {image} sh -c 'mkdir -p /old/projects/-workspace && echo chat > /old/projects/-workspace/a.jsonl && echo token > /old/.credentials.json'"
    ));
    let listed = ok(testing::run(
        OLD_CHATS,
        &["list", image, &agent, "shop"],
        &home.path,
    ));
    assert!(listed.contains(&format!("volume\t{agent}\t1")), "{listed}");
    ok(testing::run(
        OLD_CHATS,
        &["move", image, &agent, "shop"],
        &home.path,
    ));
    let moved = home.join(&format!("agents/shop/{agent}/projects/-workspace/a.jsonl"));
    assert_eq!(fs::read_to_string(moved).unwrap(), "chat\n");
    assert!(
        shell(&format!("docker volume ls -q --filter name={volume}")).is_empty(),
        "the old volume is gone"
    );
    fails(testing::run(
        OLD_CHATS,
        &["move", image, "bad name", "shop"],
        &home.path,
    ));
    fails(testing::run(
        OLD_CHATS,
        &["move", image, &agent, "../x"],
        &home.path,
    ));
}
