pub mod menu;

use std::hash::{BuildHasher, Hasher, RandomState};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{attach, config, fingerprint, remote};

const SCRIPT: &str = include_str!("scripts/services.sh");
const MAX_VERSION: usize = 40;
const MAX_NAME: usize = 64;
const MAX_PASSWORD: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mariadb,
    Redis,
    Rabbitmq,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Mariadb, Kind::Redis, Kind::Rabbitmq];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Mariadb => "mariadb",
            Kind::Redis => "redis",
            Kind::Rabbitmq => "rabbitmq",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Kind::Mariadb => "MariaDB",
            Kind::Redis => "Redis",
            Kind::Rabbitmq => "RabbitMQ",
        }
    }

    pub fn default_version(self) -> &'static str {
        match self {
            Kind::Mariadb => "10.11",
            Kind::Redis => "7",
            Kind::Rabbitmq => "3.13",
        }
    }

    fn port(self) -> u16 {
        match self {
            Kind::Mariadb => 3306,
            Kind::Redis => 6379,
            Kind::Rabbitmq => 5672,
        }
    }

    /// MariaDB and RabbitMQ have a user and a list of databases or vhosts
    pub fn has_user(self) -> bool {
        !matches!(self, Kind::Redis)
    }

    pub fn names_label(self) -> &'static str {
        match self {
            Kind::Mariadb => "databases",
            Kind::Redis => "",
            Kind::Rabbitmq => "vhosts",
        }
    }

    pub fn default_names(self) -> &'static str {
        match self {
            Kind::Mariadb => "app",
            Kind::Redis => "",
            Kind::Rabbitmq => "/",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// Stable through renames: names the data volumes
    pub id: String,
    pub kind: Kind,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// MariaDB only, set once: changing it would lock sai out of an existing database
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub root_password: String,
    /// Databases (MariaDB) or vhosts (RabbitMQ)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Stopped by the user: sai keeps it stopped until it is started again
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stopped: bool,
}

impl Service {
    pub fn new(kind: Kind) -> Self {
        let (user, password) = if kind.has_user() {
            ("dev", "dev")
        } else {
            ("", "")
        };
        Self {
            id: random_hex(8),
            kind,
            version: kind.default_version().to_owned(),
            user: user.to_owned(),
            password: password.to_owned(),
            root_password: if kind == Kind::Mariadb {
                random_hex(24)
            } else {
                String::new()
            },
            names: split_names(kind.default_names()),
            stopped: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let title = self.kind.title();
        ensure!(
            !self.version.is_empty()
                && self.version.len() <= MAX_VERSION
                && self
                    .version
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                && self
                    .version
                    .starts_with(|c: char| c.is_ascii_alphanumeric()),
            "{title}: the version can have latin letters, digits, dots and dashes, like 10.11"
        );
        ensure!(
            self.password.len() <= MAX_PASSWORD && self.password.chars().all(password_char),
            "{title}: the password can have latin letters, digits and {PASSWORD_SYMBOLS}"
        );
        if !self.kind.has_user() {
            return Ok(());
        }
        ensure!(
            !self.user.is_empty()
                && self.user.len() <= 32
                && self
                    .user
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{title}: the user can have latin letters, digits and _"
        );
        ensure!(
            !self.password.is_empty(),
            "{title}: set a password for {}",
            self.user
        );
        ensure!(
            !self.names.is_empty(),
            "{title}: name at least one of the {}",
            self.kind.names_label()
        );
        for name in &self.names {
            let ok = match self.kind {
                Kind::Mariadb => name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                _ => name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c)),
            };
            if !ok || name.len() > MAX_NAME {
                bail!(
                    "{title}: {name:?} cannot be one of the {}, use latin letters, digits and _{}",
                    self.kind.names_label(),
                    if self.kind == Kind::Rabbitmq {
                        " . / -"
                    } else {
                        ""
                    }
                );
            }
        }
        Ok(())
    }

    pub fn container(&self, project: &str) -> String {
        format!("sai-{project}-{}", self.kind.name())
    }

    pub fn image(&self) -> String {
        match self.kind {
            Kind::Mariadb => format!("mariadb:{}", self.version),
            Kind::Redis => format!("redis:{}", self.version),
            Kind::Rabbitmq => format!("rabbitmq:{}-management", self.version),
        }
    }

    fn volume_prefix(&self) -> String {
        format!("sai-svc-{}", self.id)
    }

    /// MariaDB keeps one volume per version: an upgrade starts empty instead of
    /// breaking the old data, and switching back finds it again
    fn volume(&self) -> String {
        match self.kind {
            Kind::Mariadb => format!("{}-{}", self.volume_prefix(), self.version),
            _ => self.volume_prefix(),
        }
    }

    pub fn label(&self) -> String {
        format!("{} {}", self.kind.title(), self.version)
    }

    /// How to connect, for the user and for the agent's instructions
    pub fn connection(&self) -> String {
        let port = self.kind.port();
        let names = self.names.join(", ");
        match self.kind {
            Kind::Mariadb => format!(
                "{}: localhost:{port}, user {}, password {}, databases {names} (root password {})",
                self.label(),
                self.user,
                self.password,
                self.root_password
            ),
            Kind::Redis if self.password.is_empty() => {
                format!("{}: localhost:{port}, no password", self.label())
            }
            Kind::Redis => format!(
                "{}: localhost:{port}, password {}",
                self.label(),
                self.password
            ),
            Kind::Rabbitmq => format!(
                "{}: localhost:{port} (management UI and API on localhost:15672), user {}, password {}, vhosts {names}",
                self.label(),
                self.user,
                self.password
            ),
        }
    }

    /// The command that opens the service from the agent's container
    fn client(&self) -> String {
        let port = self.kind.port();
        match self.kind {
            // The Debian client (11.x) insists on TLS, the server of the Docker image has
            // none, and the connection never leaves the container anyway
            Kind::Mariadb => format!(
                "mariadb -h 127.0.0.1 -P {port} -u {} -p{} --skip-ssl <database>",
                self.user, self.password
            ),
            Kind::Redis if self.password.is_empty() => format!("redis-cli -p {port}"),
            Kind::Redis => format!("redis-cli -p {port} -a {} --no-auth-warning", self.password),
            Kind::Rabbitmq => format!(
                "curl -u {}:{} http://localhost:15672/api/overview (the management HTTP API)",
                self.user, self.password
            ),
        }
    }

    /// What a change recreates the container for. Users, passwords, databases and vhosts
    /// are set up inside the running service, except the Redis password, which is an
    /// argument of the server
    fn spec(&self) -> String {
        let password = if self.kind == Kind::Redis {
            self.password.as_str()
        } else {
            ""
        };
        fingerprint::of(&format!(
            "{}\n{}\n{}\n{password}\n{SCRIPT}",
            self.kind.name(),
            self.image(),
            self.volume()
        ))
    }

    fn args(&self, project: &str) -> Vec<String> {
        vec![
            self.container(project),
            self.kind.name().to_owned(),
            self.image(),
            self.volume_prefix(),
            self.volume(),
            self.user.clone(),
            self.password.clone(),
            self.root_password.clone(),
            self.names.join(","),
            if self.stopped { "no" } else { "yes" }.to_owned(),
            self.spec(),
        ]
    }
}

const PASSWORD_SYMBOLS: &str = "._-+=@%:,!#^*~";

fn password_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || PASSWORD_SYMBOLS.contains(c)
}

pub fn split_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.dedup();
    names
}

/// Ids and the MariaDB root password. RandomState is seeded from the system's randomness
fn random_hex(length: usize) -> String {
    let mut text = String::new();
    while text.len() < length {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(text.len());
        text.push_str(&format!("{:016x}", hasher.finish()));
    }
    text.truncate(length);
    text
}

/// The step of `sai chat` that starts, updates and removes the project's services
pub fn ensure_step(project: &str, services: &[Service]) -> (&'static str, Vec<String>) {
    let mut args = vec!["ensure".to_owned(), project.to_owned()];
    for service in services {
        args.extend(service.args(project));
    }
    (SCRIPT, args)
}

/// Brings the VPS in line with the project's list: the agent's instructions, then the
/// services themselves. Without a running agent they start with the next `sai chat`
pub fn apply(project: &str, services: &[Service]) -> Result<()> {
    remote::run_all(
        "update the services",
        &[
            attach::instructions_step(project, &config::instructions()?, services),
            ensure_step(project, services),
        ],
    )
}

pub fn restart(project: &str, service: &Service) -> Result<()> {
    remote::run(SCRIPT, &["restart", project, &service.container(project)])
}

/// Container name and Docker state (running, exited, ...) of each service
pub fn status(project: &str) -> Result<Vec<(String, String)>> {
    Ok(remote::output(SCRIPT, &["status", project])?
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, state)| (name.trim().to_owned(), state.trim().to_owned()))
        .collect())
}

/// Removes a service with its data
pub fn remove(project: &str, service: &Service) -> Result<()> {
    remote::run(
        SCRIPT,
        &[
            "remove",
            project,
            &service.container(project),
            &service.volume_prefix(),
        ],
    )
}

/// Removes all services of a project with their data
pub fn purge(project: &str) -> Result<()> {
    remote::run(SCRIPT, &["purge", project])
}

/// The part of the agent's instructions that tells it how to reach the services
pub fn instructions(services: &[Service]) -> String {
    let running: Vec<&Service> = services.iter().filter(|service| !service.stopped).collect();
    if running.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "\n\n## Services of this project\n\nThese run next to you and are reachable on localhost. Their data is not part of the project snapshots. You cannot start, stop or reconfigure them, the user does that in sai. To look into them, use the clients installed in your container rather than installing a driver just for that (the project's own dependencies install as usual):\n\n",
    );
    for service in running {
        text.push_str(&format!(
            "- {}\n  `{}`\n",
            service.connection(),
            service.client()
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{Kind, Service, split_names};

    #[test]
    fn templates_are_valid() {
        for kind in Kind::ALL {
            let service = Service::new(kind);
            service.validate().unwrap();
            assert_eq!(service.id.len(), 8);
        }
        assert_eq!(Service::new(Kind::Mariadb).root_password.len(), 24);
        assert!(Service::new(Kind::Redis).root_password.is_empty());
    }

    #[test]
    fn refuses_values_that_could_escape_quotes() {
        let mut service = Service::new(Kind::Mariadb);
        service.password = "x' OR '1".to_owned();
        assert!(service.validate().is_err());
        service.password = "dev".to_owned();
        service.names = vec!["app`; DROP".to_owned()];
        assert!(service.validate().is_err());
        service.names = vec!["rs_server".to_owned()];
        service.version = "10.11;rm".to_owned();
        assert!(service.validate().is_err());
        service.version = "10.11.8".to_owned();
        service.validate().unwrap();
        let mut rabbit = Service::new(Kind::Rabbitmq);
        rabbit.names = split_names("tos, /");
        rabbit.validate().unwrap();
        rabbit.user = "a b".to_owned();
        assert!(rabbit.validate().is_err());
    }

    #[test]
    fn mariadb_keeps_one_volume_per_version() {
        let mut service = Service::new(Kind::Mariadb);
        let old = service.volume();
        service.version = "11.4".to_owned();
        assert_ne!(old, service.volume());
        assert!(service.volume().starts_with(&service.volume_prefix()));
        let redis = Service::new(Kind::Redis);
        assert_eq!(redis.volume(), redis.volume_prefix());
    }

    #[test]
    fn renaming_the_project_keeps_the_data() {
        let service = Service::new(Kind::Rabbitmq);
        assert_ne!(service.container("a"), service.container("b"));
        assert_eq!(service.args("a")[3], service.args("b")[3]);
        assert_eq!(service.args("a")[4], service.args("b")[4]);
    }

    #[test]
    fn tells_the_agent_how_to_connect() {
        let mut redis = Service::new(Kind::Redis);
        redis.password = "secret".to_owned();
        let mut stopped = Service::new(Kind::Mariadb);
        stopped.stopped = true;
        let text = super::instructions(&[redis, stopped]);
        assert!(text.contains("redis-cli -p 6379 -a secret"));
        let mariadb = super::instructions(&[Service::new(Kind::Mariadb)]);
        assert!(mariadb.contains("--skip-ssl"));
        assert!(!text.contains("MariaDB"));
        assert!(super::instructions(&[]).is_empty());
    }

    #[test]
    fn splits_names_by_commas_and_spaces() {
        assert_eq!(
            split_names("test, rs_server  app"),
            ["test", "rs_server", "app"]
        );
        assert!(split_names(" , ").is_empty());
    }
}
