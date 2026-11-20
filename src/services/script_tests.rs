//! The services script run for real: the memory budget always, the whole life of the
//! services with Docker when SAI_DOCKER_TESTS is set (CI on Linux)
use super::{Kind, SCRIPT, Service, ensure_step};
use crate::testing::{self, TempDir, fails, ok, shell, unique};

const MEMORY: &str = include_str!("scripts/memory.sh");
const STOP: &str = include_str!("../project/scripts/stop.sh");
const MIB: u64 = 1024;

/// Budget figures in KiB for a VPS of `gb` gigabytes
fn budget(gb: u64) -> Vec<u64> {
    let home = TempDir::new("memory");
    let script = format!(
        "{MEMORY}\ngb=$1\nmemory_total() {{ echo $(( gb * 1048576 )); }}\n\
         echo $(memory_system) $(memory_agent) $(memory_agent mariadb rabbitmq redis) \
         $(memory_left mariadb rabbitmq redis) $(memory_service redis) $(memory_swap 1000)"
    );
    ok(testing::run(&script, &[&gb.to_string()], &home.path))
        .split_whitespace()
        .map(|number| number.parse().unwrap())
        .collect()
}

#[test]
fn memory_budget_keeps_room_for_the_system() {
    let [system, alone, with_all, left, redis, swap] = budget(2)[..] else {
        panic!("six numbers");
    };
    assert_eq!(system, 512 * MIB, "at least 512 MB stay with the system");
    assert_eq!(alone, 1536 * MIB);
    assert_eq!(
        left,
        896 * MIB,
        "a 2 GB VPS is short with all three services"
    );
    assert_eq!(with_all, 1024 * MIB, "the agent never gets less than 1 GB");
    assert_eq!(redis, 128 * MIB);
    assert_eq!(swap, 1500, "swap is at most half of the limit");

    let [system, _, with_all, left, _, _] = budget(8)[..] else {
        panic!("six numbers");
    };
    assert_eq!(system, 8 * 1024 * MIB * 15 / 100);
    assert_eq!(with_all, left, "a bigger VPS has room for everything");
    assert!(system + with_all + 2 * 1024 * MIB + 512 * MIB <= 8 * 1024 * MIB);
}

/// Removes everything a Docker test made, also when it fails halfway
struct Cleanup {
    project: String,
    ids: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let project = &self.project;
        shell(&format!(
            "docker ps -aq --filter label=sai.service-of={project} | xargs -r docker rm -f; docker rm -f sai-{project}"
        ));
        for id in &self.ids {
            shell(&format!(
                "docker volume ls -q --filter name=sai-svc-{id} | xargs -r docker volume rm"
            ));
        }
    }
}

fn agent(project: &str) {
    shell(&format!(
        "docker run -d --name sai-{project} debian:trixie-slim sleep infinity"
    ));
}

fn ensure(project: &str, services: &[Service], home: &TempDir) -> String {
    let (script, args) = ensure_step(project, services);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    ok(testing::run(script, &args, &home.path))
}

fn reachable(project: &str, port: u16) -> bool {
    shell(&format!(
        "docker exec sai-{project} bash -c 'exec 3<>/dev/tcp/127.0.0.1/{port}' && echo yes"
    )) == "yes"
}

fn running(container: &str) -> bool {
    shell(&format!(
        "docker inspect -f '{{{{.State.Running}}}}' {container}"
    )) == "true"
}

fn limits(container: &str) -> (u64, u64) {
    let found = shell(&format!(
        "docker inspect -f '{{{{.HostConfig.Memory}}}} {{{{.HostConfig.MemorySwap}}}}' {container}"
    ));
    let mut numbers = found.split_whitespace().map(|n| n.parse::<u64>().unwrap());
    (numbers.next().unwrap(), numbers.next().unwrap())
}

fn databases(project: &str) -> String {
    shell(&format!(
        "docker exec sai-{project}-mariadb mariadb -h 127.0.0.1 -u tos -ppw-1 --skip-ssl -e 'SHOW DATABASES'"
    ))
}

#[test]
fn services_live_next_to_the_agent() {
    if !testing::docker() {
        return;
    }
    let project = unique("svc");
    let home = TempDir::new("services");
    let mut mariadb = Service::new(Kind::Mariadb);
    mariadb.user = "tos".to_owned();
    mariadb.password = "pw-1".to_owned();
    mariadb.names = vec!["test".to_owned(), "rs_server".to_owned()];
    let mut redis = Service::new(Kind::Redis);
    redis.password = "secret".to_owned();
    let mut rabbit = Service::new(Kind::Rabbitmq);
    rabbit.user = "tos".to_owned();
    rabbit.password = "pw-2".to_owned();
    rabbit.names = vec!["tos".to_owned(), "/".to_owned()];
    let mut services = vec![mariadb, redis, rabbit];
    let _cleanup = Cleanup {
        project: project.clone(),
        ids: services.iter().map(|service| service.id.clone()).collect(),
    };
    agent(&project);

    let started = ensure(&project, &services, &home);
    assert!(!started.contains("warning"), "{started}");
    let found = databases(&project);
    assert!(
        found.contains("test") && found.contains("rs_server"),
        "{found}"
    );
    assert_eq!(
        shell(&format!(
            "docker exec sai-{project}-redis redis-cli -a secret --no-auth-warning ping"
        )),
        "PONG"
    );
    assert!(
        shell(&format!("docker exec sai-{project}-redis redis-cli ping")).contains("NOAUTH"),
        "Redis asks for the password"
    );
    for vhost in ["tos", "/"] {
        let permissions = shell(&format!(
            "docker exec sai-{project}-rabbitmq rabbitmqctl -q list_permissions -p {vhost}"
        ));
        assert!(permissions.contains("tos"), "{vhost}: {permissions}");
    }
    for port in [3306, 6379, 5672] {
        assert!(reachable(&project, port), "the agent reaches {port}");
    }

    let (memory, swap) = limits(&format!("sai-{project}"));
    assert!(
        memory > 0 && swap > memory && swap <= memory * 3 / 2 + 1024,
        "{memory} {swap}"
    );
    let (memory, swap) = limits(&format!("sai-{project}-redis"));
    assert!(
        memory > 0 && swap > memory && swap <= memory * 3 / 2 + 1024,
        "{memory} {swap}"
    );

    let mariadb_id = shell(&format!(
        "docker inspect -f '{{{{.Id}}}}' sai-{project}-mariadb"
    ));
    let again = ensure(&project, &services, &home);
    assert!(!again.contains("Starting"), "nothing to do: {again}");
    services[0].names.push("extra".to_owned());
    ensure(&project, &services, &home);
    assert!(
        databases(&project).contains("extra"),
        "a new database without a restart"
    );
    assert_eq!(
        shell(&format!(
            "docker inspect -f '{{{{.Id}}}}' sai-{project}-mariadb"
        )),
        mariadb_id
    );

    // A new agent container, as after an update of sai: services follow it
    shell(&format!(
        "docker ps -aq --filter label=sai.service-of={project} | xargs -r docker rm -f; docker rm -f sai-{project}"
    ));
    agent(&project);
    ensure(&project, &services, &home);
    assert!(
        databases(&project).contains("rs_server"),
        "the data survives"
    );
    for port in [3306, 6379, 5672] {
        assert!(reachable(&project, port), "the new agent reaches {port}");
    }

    // sai stop, then sai chat
    ok(testing::run(STOP, &[&format!("sai-{project}")], &home.path));
    assert!(!running(&format!("sai-{project}-mariadb")));
    assert!(!running(&format!("sai-{project}")));
    shell(&format!("docker start sai-{project}"));
    ensure(&project, &services, &home);
    assert!(reachable(&project, 3306), "back after a stop");

    // Stopped by the user in the menu: stays stopped
    services[1].stopped = true;
    ensure(&project, &services, &home);
    assert!(!running(&format!("sai-{project}-redis")));
    assert!(!reachable(&project, 6379));

    // Removed with its data
    let removed = services.remove(0);
    let container = removed.container(&project);
    let prefix = removed.volume_prefix();
    ok(testing::run(
        SCRIPT,
        &["remove", &project, &container, &prefix],
        &home.path,
    ));
    assert!(shell(&format!("docker ps -aq --filter name={container}")).is_empty());
    assert!(shell(&format!("docker volume ls -q --filter name={prefix}")).is_empty());
    fails(testing::run(
        SCRIPT,
        &["remove", &project, "sai-other-mariadb", &prefix],
        &home.path,
    ));

    ok(testing::run(SCRIPT, &["purge", &project], &home.path));
    assert!(
        shell(&format!(
            "docker ps -aq --filter label=sai.service-of={project}"
        ))
        .is_empty()
    );
}
