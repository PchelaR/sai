use anyhow::{Result, bail, ensure};

use crate::agent::Agent;
use crate::logs::time;
use crate::remote;

const CHATS: &str = concat!(include_str!("agent_state.sh"), include_str!("chats.sh"));
const BUSY: &[&str] = &["working", "asking"];

pub struct Chat {
    pub id: String,
    pub modified: u64,
    pub title: String,
}

impl Chat {
    pub fn label(&self) -> String {
        format!("{}  ·  {}", self.title, time::describe(self.modified).0)
    }
}

pub struct Chats {
    pub state: String,
    pub clients: usize,
    pub list: Vec<Chat>,
}

impl Chats {
    pub fn busy(&self) -> bool {
        BUSY.contains(&self.state.as_str())
    }

    pub fn running(&self) -> Option<&Chat> {
        (self.state != "idle").then(|| self.list.first()).flatten()
    }
}

pub fn load(project: &str, agent: Agent) -> Result<Chats> {
    let text = remote::output(CHATS, &["list", &container(project), agent.name(), project])?;
    Ok(parse(&text))
}

pub fn state(project: &str, agent: Agent) -> Result<Chats> {
    let text = remote::output(
        CHATS,
        &["state", &container(project), agent.name(), project],
    )?;
    Ok(parse(&text))
}

pub fn ensure_idle(project: &str, agent: Agent) -> Result<()> {
    let current = state(project, agent)?;
    ensure!(
        !current.busy(),
        "the agent in {project} is busy with a task, try again when it finishes"
    );
    Ok(())
}

pub fn show(project: &str, agent: Agent, id: &str) -> Result<String> {
    ensure!(valid_id(id), "invalid chat id {id:?}");
    remote::output(
        CHATS,
        &["show", &container(project), agent.name(), project, id],
    )
}

pub fn stop_if_idle(project: &str, agent: Agent) -> Result<()> {
    remote::run(
        CHATS,
        &["switch", &container(project), agent.name(), project],
    )
}

pub fn remove(project: &str, agent: Agent, id: &str) -> Result<()> {
    ensure!(valid_id(id), "invalid chat id {id:?}");
    remote::run(
        CHATS,
        &["remove", &container(project), agent.name(), project, id],
    )
}

pub fn pick(chats: &Chats, number: usize) -> Result<&Chat> {
    match chats.list.get(number.wrapping_sub(1)) {
        Some(chat) => Ok(chat),
        None => bail!("there is no chat number {number}, see the list with `sai chats`"),
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn container(project: &str) -> String {
    crate::project::id(project)
}

fn parse(text: &str) -> Chats {
    let mut chats = Chats {
        state: "idle".to_owned(),
        clients: 0,
        list: Vec::new(),
    };
    for line in text.lines() {
        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        match fields.as_slice() {
            ["state", state] => chats.state = state.trim().to_owned(),
            ["clients", count] => chats.clients = count.trim().parse().unwrap_or_default(),
            ["chat", id, modified, title] if valid_id(id) => chats.list.push(Chat {
                id: (*id).to_owned(),
                modified: modified.parse().unwrap_or_default(),
                title: (*title).to_owned(),
            }),
            _ => {}
        }
    }
    chats
}

#[cfg(test)]
mod tests {
    use super::{parse, valid_id};

    #[test]
    fn reads_the_chat_list() {
        let chats = parse(
            "state\tworking\nclients\t2\nchat\tab-12\t1700000000\tFix login\nchat\tbad id!\t1\tx\nchat\tcd-34\t1600000000\tAdd webhooks\n",
        );
        assert!(chats.busy());
        assert_eq!(chats.clients, 2);
        assert_eq!(chats.list.len(), 2);
        assert_eq!(chats.running().map(|chat| chat.id.as_str()), Some("ab-12"));
        assert_eq!(chats.list[1].title, "Add webhooks");
        let idle = parse("state\tidle\nchat\tab-12\t1\tx\n");
        assert!(!idle.busy());
        assert!(idle.running().is_none());
    }

    #[test]
    fn accepts_only_simple_ids() {
        assert!(valid_id("8f14e45f-ceea-467a-9575-7f4b8f3a1c2d"));
        assert!(!valid_id("x; rm -rf /"));
        assert!(!valid_id(""));
    }
}
