pub struct Step {
    pub title: &'static str,
    pub script: &'static str,
}

macro_rules! step {
    ($title:literal, $file:literal) => {
        Step {
            title: $title,
            script: concat!(
                include_str!("scripts/prelude.sh"),
                include_str!("../image/build.sh"),
                include_str!(concat!("scripts/", $file))
            ),
        }
    };
}

pub const PREPARE: &[Step] = &[
    step!("Checking OS", "check_os.sh"),
    step!(
        "Installing packages (may take a few minutes)",
        "packages.sh"
    ),
    step!("Configuring swap and journald", "swap.sh"),
    step!("Creating user", "user.sh"),
    step!("Installing Docker", "docker.sh"),
    step!(
        "Creating isolated container network",
        "container_network.sh"
    ),
    step!("Opening new ssh port", "ssh_open_port.sh"),
    step!("Enabling firewall", "firewall.sh"),
    step!("Configuring fail2ban", "fail2ban.sh"),
    step!("Checking container isolation", "container_isolation.sh"),
    step!(
        "Building base image (may take up to an hour)",
        "base_image.sh"
    ),
];

pub const LOCK_DOWN: &[Step] = &[step!("Locking down ssh", "ssh_lock.sh")];

pub const ROLLBACK: &[Step] = &[step!("Rolling back ssh changes", "ssh_rollback.sh")];

#[cfg(test)]
mod tests {
    use super::{LOCK_DOWN, PREPARE, ROLLBACK};

    #[test]
    fn scripts_use_unix_line_endings() {
        for step in PREPARE.iter().chain(LOCK_DOWN).chain(ROLLBACK) {
            assert!(!step.script.contains('\r'), "{} has CRLF", step.title);
        }
    }
}
