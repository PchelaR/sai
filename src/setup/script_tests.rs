//! The setup of a VPS run for real on this machine, exactly the scripts `sai setup` sends.
//! It changes the machine for good: sshd, the firewall, users. So it runs only where
//! SAI_VPS_TESTS is set, on a throwaway Ubuntu runner in CI
use std::fs;
use std::process::Command;

use super::steps::{LOCK_DOWN, PREPARE, ROLLBACK, SUMMARY, Step};
use crate::testing::{TempDir, shell};
use crate::vps::{MINIMAL, SETUP_MARKER, SSH_PORT, USER};
use crate::{image, vps};

/// Runs a step the way the ssh session does: as root, with the same variables
fn run(step: &Step, pubkey: &str) -> String {
    let dir = TempDir::new("setup-step");
    let script = dir.join("step.sh");
    fs::write(&script, step.script).unwrap();
    let output = Command::new("sudo")
        .args(["-n", "env"])
        .arg(format!("SAI_USER={USER}"))
        .arg(format!("SAI_PORT={SSH_PORT}"))
        .arg(format!("SAI_MINIMAL={MINIMAL}"))
        .arg(format!("SAI_PUBKEY={pubkey}"))
        .arg(format!("SAI_SETUP_MARKER={SETUP_MARKER}"))
        .arg(format!(
            "SAI_BUILD_CACHE_MARKER={}",
            vps::build_cache_marker()
        ))
        .arg(format!("SAI_IMAGE={}", image::base()))
        .arg(format!("SAI_DOCKERFILE={}", image::encoded_base()))
        .arg("bash")
        .arg(&script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "step failed: {}\nstdout:\n{}\nstderr:\n{}",
        step.title,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn login(key: &str, port: u16) -> String {
    shell(&format!(
        "ssh -p {port} -i {key} -o BatchMode=yes -o StrictHostKeyChecking=no \
         -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 {USER}@127.0.0.1 echo logged-in 2>&1"
    ))
}

/// Whether ufw lets this port in, read by rule: 28522/tcp must not count as 22/tcp
fn allowed(port: u16) -> bool {
    shell("sudo ufw status")
        .lines()
        .any(|line| line.starts_with(&format!("{port}/tcp ")) && line.contains("ALLOW"))
}

fn listening(port: u16) -> bool {
    !shell(&format!("ss -ltnH 'sport = :{port}'")).is_empty()
}

#[test]
fn sets_up_and_locks_down_a_fresh_vps() {
    if std::env::var_os("SAI_VPS_TESTS").is_none() {
        return;
    }
    let keys = TempDir::new("setup-keys");
    let key = keys.join("id_ed25519");
    let key = key.to_str().unwrap();
    shell(&format!("ssh-keygen -q -t ed25519 -N '' -f {key}"));
    let pubkey = fs::read_to_string(format!("{key}.pub")).unwrap();
    let pubkey = pubkey.trim();

    // Updating the system and building the base image take long and need nothing checked
    for step in PREPARE
        .iter()
        .filter(|step| !step.title.starts_with("Updating") && !step.title.starts_with("Building"))
    {
        run(step, pubkey);
    }
    assert_eq!(shell(&format!("id -un {USER}")), USER);
    assert!(
        shell(&format!("sudo cat /home/{USER}/.ssh/authorized_keys")).contains(pubkey),
        "the user logs in with the key"
    );
    assert!(
        listening(22) && listening(SSH_PORT),
        "both ports until the lockdown"
    );
    assert!(shell("sudo ufw status").contains("Status: active"));
    assert!(allowed(SSH_PORT) && allowed(22));
    assert!(shell("sudo fail2ban-client status sshd").contains("Currently banned"));
    assert_eq!(
        shell(
            "docker network inspect -f '{{index .Options \"com.docker.network.bridge.enable_icc\"}}' sai-net"
        ),
        "false",
        "agents of different projects cannot reach each other"
    );
    let rules = shell("sudo iptables -S DOCKER-USER; sudo iptables -S INPUT");
    let dropped = |chain: &str, net: &str| {
        rules.lines().any(|rule| {
            rule.starts_with(&format!("-A {chain} "))
                && rule.contains("-i br-sai")
                && rule.contains(net)
                && rule.ends_with("-j DROP")
        })
    };
    for net in [
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "169.254.0.0/16",
    ] {
        assert!(
            dropped("DOCKER-USER", net),
            "agents cannot reach {net}:\n{rules}"
        );
    }
    assert!(
        dropped("INPUT", ""),
        "agents cannot reach the VPS itself:\n{rules}"
    );
    assert!(
        !shell("swapon --show --noheadings").is_empty(),
        "swap is on"
    );
    assert_eq!(login(key, SSH_PORT), "logged-in");

    // Run twice, as after a failed setup: nothing breaks
    run(&PREPARE[0], pubkey);
    for step in PREPARE
        .iter()
        .filter(|step| step.title.starts_with("Creating") || step.title.starts_with("Opening"))
    {
        run(step, pubkey);
    }
    assert_eq!(login(key, SSH_PORT), "logged-in");

    for step in LOCK_DOWN {
        run(step, pubkey);
    }
    assert_eq!(login(key, SSH_PORT), "logged-in");
    assert!(!listening(22), "only the sai port is left");
    let sshd = shell("sudo sshd -T");
    for setting in [
        "passwordauthentication no",
        "permitrootlogin no",
        &format!("allowusers {USER}"),
        "allowtcpforwarding no",
    ] {
        assert!(sshd.contains(setting), "{setting}");
    }
    assert_eq!(
        shell("sudo passwd -S root").split_whitespace().nth(1),
        Some("L"),
        "root is locked"
    );
    assert!(!allowed(22) && allowed(SSH_PORT), "port 22 is closed");
    assert_eq!(
        shell(&format!("sudo test -f {SETUP_MARKER} && echo yes")),
        "yes"
    );

    let summary = run(&SUMMARY, pubkey);
    for line in [
        format!("ssh port\t{SSH_PORT}"),
        format!("ssh login\tonly {USER} with the key, passwords and root are disabled"),
        "fail2ban\ton, 3 failed logins ban the IP for 60 min".to_owned(),
    ] {
        assert!(summary.contains(&line), "{line} in\n{summary}");
    }
    assert!(summary.contains("firewall (ufw)\ton, open:"), "{summary}");

    // A lockdown that failed is undone: the usual port works again
    for step in ROLLBACK {
        run(step, pubkey);
    }
    assert_eq!(
        shell(&format!("sudo test -e {SETUP_MARKER} || echo gone")),
        "gone"
    );
    assert!(allowed(22));
    assert_eq!(login(key, 22), "logged-in");
}
