//! Scripts of `sai chat` run for real. Linux only, Docker ones with SAI_DOCKER_TESTS
use std::fs;
use std::os::unix::fs::MetadataExt;

use base64::{Engine, prelude::BASE64_STANDARD};

use super::{ENSURE_CONTAINER, OLD_CHATS, WRITE_INSTRUCTIONS};
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

/// Removes what a Docker test made, also when it fails halfway
#[derive(Default)]
struct Cleanup {
    containers: Vec<String>,
    images: Vec<String>,
    volumes: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for container in &self.containers {
            shell(&format!("docker rm -f {container}"));
        }
        for image in &self.images {
            shell(&format!("docker rmi -f {image}"));
        }
        for volume in &self.volumes {
            shell(&format!("docker volume rm -f {volume}"));
        }
    }
}

/// An agent image in miniature: it keeps running, and its tmux says whether a chat is open
fn agent_image(tag: &str, variant: &str) {
    shell(&format!(
        r#"docker build -q -t {tag} - <<'END'
FROM debian:trixie-slim
RUN printf '#!/bin/sh\n[ -e /tmp/chat ]\n' > /usr/local/bin/tmux && chmod +x /usr/local/bin/tmux
LABEL variant={variant}
CMD ["sleep", "infinity"]
END"#
    ));
    assert!(
        !shell(&format!("docker image inspect -f '{{{{.Id}}}}' {tag}")).is_empty(),
        "cannot build {tag}"
    );
}

fn inspect(container: &str, format: &str) -> String {
    shell(&format!("docker inspect -f '{format}' {container}"))
}

#[test]
fn the_agent_container_is_created_kept_and_replaced_when_it_changed() {
    if !testing::docker() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let home = TempDir::new("container");
    let project = unique("box");
    let container = format!("sai-{project}");
    let repo = unique("sai-test-agent");
    let (first, second) = (format!("{repo}:a"), format!("{repo}:b"));
    cleanup.containers.push(container.clone());
    cleanup.images.extend([first.clone(), second.clone()]);
    shell("docker network inspect sai-net >/dev/null 2>&1 || docker network create sai-net");
    agent_image(&first, "a");
    agent_image(&second, "b");

    let workspace = home.join("projects/box");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(workspace.join("main.rs"), "fn main() {}").unwrap();
    let policy = home.join("policy.json");
    fs::write(&policy, "{}").unwrap();
    let report = home.join("turn-report");
    fs::write(&report, "report").unwrap();
    // A setting shared like the sign-in by an older sai
    let state = home.join(&format!("agents/{project}/claude"));
    let login = home.join("agents/.login/claude");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&login).unwrap();
    fs::write(login.join("settings.json"), "shared").unwrap();
    std::os::unix::fs::symlink("/login/settings.json", state.join("settings.json")).unwrap();

    let mounts = [
        format!(
            "{}:/etc/claude-code/managed-settings.json",
            policy.display()
        ),
        format!("{}:/state/sai-turn/report", report.display()),
    ];
    let ensure = |image: &str, spec: &str| {
        let mut args = vec![
            container.as_str(),
            workspace.to_str().unwrap(),
            project.as_str(),
            image,
            spec,
            "claude",
            ".credentials.json",
            "settings.json",
        ];
        args.extend(mounts.iter().map(String::as_str));
        ok(testing::run(ENSURE_CONTAINER, &args, &home.path))
    };
    let id = || inspect(&container, "{{.Id}}");
    let exec = |command: &str| {
        shell(&format!(
            "docker exec {container} sh -c '{command}' 2>&1; echo \" $?\""
        ))
    };

    // Created, locked down, with the project and the agent's files
    ensure(&first, "one");
    assert_eq!(inspect(&container, "{{.State.Running}}"), "true");
    assert_eq!(
        inspect(&container, "{{index .Config.Labels \"sai.spec\"}}"),
        "one"
    );
    assert_eq!(
        inspect(&container, "{{index .Config.Labels \"sai.project\"}}"),
        project
    );
    assert_eq!(inspect(&container, "{{.HostConfig.CapDrop}}"), "[ALL]");
    assert!(inspect(&container, "{{.HostConfig.SecurityOpt}}").contains("no-new-privileges"));
    assert!(inspect(&container, "{{json .NetworkSettings.Networks}}").contains("sai-net"));
    let memory: u64 = inspect(&container, "{{.HostConfig.Memory}}")
        .parse()
        .unwrap();
    let swap: u64 = inspect(&container, "{{.HostConfig.MemorySwap}}")
        .parse()
        .unwrap();
    assert!(memory >= 1024 * 1024 * 1024, "at least 1 GB for the agent");
    assert!(
        swap.abs_diff(memory + memory / 2) <= 1024,
        "swap is at most half of the limit"
    );
    assert_eq!(exec("cat /workspace/main.rs"), "fn main() {} 0");
    assert_eq!(exec("cat /state/sai-turn/report"), "report 0");
    assert!(
        !exec("echo x > /etc/claude-code/managed-settings.json").ends_with(" 0"),
        "the agent cannot change its policy"
    );
    assert_eq!(
        fs::read_link(state.join(".credentials.json"))
            .unwrap()
            .to_str(),
        Some("/login/.credentials.json"),
        "the sign-in is shared between projects"
    );
    assert!(
        fs::symlink_metadata(state.join("settings.json"))
            .unwrap()
            .is_file(),
        "settings are each project's own now"
    );
    assert_eq!(
        fs::read_to_string(state.join("settings.json")).unwrap(),
        "shared"
    );
    assert!(
        fs::read_to_string(state.join(".claude.json"))
            .unwrap()
            .contains("hasCompletedOnboarding")
    );

    // Nothing changed: the same container, also after a stop
    let created = id();
    let again = ensure(&first, "one");
    assert!(!again.contains("Updating"), "{again}");
    assert_eq!(id(), created);
    shell(&format!("docker stop -t 0 {container}"));
    ensure(&first, "one");
    assert_eq!(id(), created);
    assert_eq!(inspect(&container, "{{.State.Running}}"), "true");

    // Changed while a chat is open: the chat is not cut off
    exec("touch /tmp/chat");
    let waiting = ensure(&first, "two");
    assert!(
        waiting.contains("after all agent sessions end"),
        "{waiting}"
    );
    assert_eq!(id(), created);

    // Changed with no chat: replaced, and its services go with it to follow the new one
    exec("rm /tmp/chat");
    let service = format!("{container}-redis");
    cleanup.containers.push(service.clone());
    shell(&format!(
        "docker run -d --name {service} --label sai.service-of={project} {first} sleep infinity"
    ));
    let updated = ensure(&first, "two");
    assert!(updated.contains("Updating agent container"), "{updated}");
    assert_ne!(id(), created);
    assert_eq!(
        inspect(&container, "{{index .Config.Labels \"sai.spec\"}}"),
        "two"
    );
    assert!(
        inspect(&service, "{{.Id}}").is_empty(),
        "services start again after the agent"
    );
    assert_eq!(
        exec("cat /workspace/main.rs"),
        "fn main() {} 0",
        "the project is still there"
    );

    // A new image: replaced, and the old one of the same agent goes
    let before = id();
    ensure(&second, "two");
    assert_ne!(id(), before);
    assert!(
        shell(&format!("docker image inspect -f '{{{{.Id}}}}' {first}")).is_empty(),
        "the old image is removed"
    );
}

#[test]
fn the_sign_in_moves_out_of_the_old_shared_volume() {
    if !testing::docker() {
        return;
    }
    let mut cleanup = Cleanup::default();
    let home = TempDir::new("login");
    let agent = unique("agent");
    let project = unique("signin");
    let container = format!("sai-{project}");
    let image = format!("{}:a", unique("sai-test-login"));
    let volume = format!("sai-state-{agent}");
    cleanup.containers.push(container.clone());
    cleanup.images.push(image.clone());
    cleanup.volumes.push(volume.clone());
    shell("docker network inspect sai-net >/dev/null 2>&1 || docker network create sai-net");
    agent_image(&image, "login");
    shell(&format!(
        "docker run --rm -v {volume}:/old debian:trixie-slim sh -c 'echo token > /old/auth.json'"
    ));
    let workspace = home.join("projects/signin");
    fs::create_dir_all(&workspace).unwrap();
    ok(testing::run(
        ENSURE_CONTAINER,
        &[
            &container,
            workspace.to_str().unwrap(),
            &project,
            &image,
            "one",
            &agent,
            "auth.json",
            "",
        ],
        &home.path,
    ));
    let login = home.join(&format!("agents/.login/{agent}/auth.json"));
    assert_eq!(fs::read_to_string(login).unwrap(), "token\n");
    assert!(
        !home
            .join(&format!("agents/{project}/{agent}/.claude.json"))
            .exists(),
        "only Claude needs its onboarding skipped"
    );
}
