use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::agent::Agent;
use crate::project::folders::{self, Plan};
use crate::project::{Registry, change, remove, rename, stop};
use crate::{chat, doctor, logs, output, reset, shell, snapshot, update, upgrade};

pub enum Action {
    Open(Session),
    Run(Task),
}

pub enum Session {
    Chat(String),
    Shell(String),
    Vps,
}

pub enum Task {
    Folders {
        project: String,
        agent: Option<Agent>,
        plan: Plan,
        git: bool,
    },
    Change(String, Agent),
    Rename {
        project: String,
        name: String,
    },
    Restore {
        project: String,
        snapshot: String,
        target: Option<String>,
    },
    Stop(String),
    Remove {
        name: String,
        purge: bool,
    },
    Update,
    Upgrade,
    Doctor,
    Logs,
    Reset,
}

pub enum Progress {
    Line(String),
    Done(Result<(), String>),
}

impl Task {
    pub fn title(&self) -> String {
        match self {
            Task::Folders { project, .. } => format!("folders {project}"),
            Task::Change(project, agent) => format!("change {agent} {project}"),
            Task::Rename { project, name } => format!("rename {project} {name}"),
            Task::Restore { snapshot, .. } => format!("restore {snapshot}"),
            Task::Stop(name) => format!("stop {name}"),
            Task::Remove { name, purge: false } => format!("rm {name}"),
            Task::Remove { name, purge: true } => format!("rm {name} --purge"),
            Task::Update => "update".to_owned(),
            Task::Upgrade => "upgrade".to_owned(),
            Task::Doctor => "doctor".to_owned(),
            Task::Logs => "logs".to_owned(),
            Task::Reset => "reset".to_owned(),
        }
    }
}

pub fn open(terminal: &mut DefaultTerminal, session: Session) -> Result<()> {
    super::restore();
    let result = match session {
        Session::Chat(project) => chat::run(Some(&project), None),
        Session::Shell(project) => shell::run(Some(&project)),
        Session::Vps => shell::vps(),
    };
    if let Err(error) = &result {
        logs::error(error);
        eprintln!("Error: {error:#}");
        wait_for_enter()?;
    }
    *terminal = super::init()?;
    Ok(())
}

pub fn spawn(task: Task) -> Receiver<Progress> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let lines = sender.clone();
        output::capture(move |line| {
            lines.send(Progress::Line(line)).ok();
        });
        let result = execute(task).map_err(|error| {
            logs::error(&error);
            format!("{error:#}")
        });
        sender.send(Progress::Done(result)).ok();
    });
    receiver
}

fn execute(task: Task) -> Result<()> {
    match task {
        Task::Folders {
            project,
            agent,
            plan,
            git,
        } => folders::apply(&project, agent, plan, git),
        Task::Change(project, agent) => change::run(Some(&project), agent),
        Task::Rename { project, name } => rename::run(&project, &name),
        Task::Restore {
            project,
            snapshot,
            target,
        } => {
            let registry = Registry::open()?;
            snapshot::restore(registry.get(&project)?, &snapshot, target.as_deref())
        }
        Task::Stop(name) => stop::run(Some(&name)),
        Task::Remove { name, purge } => remove::run(&name, purge),
        Task::Update => update::run(),
        Task::Upgrade => upgrade::run(),
        Task::Doctor => doctor::run(),
        Task::Logs => logs::report(),
        Task::Reset => reset::execute(false),
    }
}

fn wait_for_enter() -> Result<()> {
    print!("\nPress Enter to return to sai");
    io::stdout().flush()?;
    io::stdin().read_line(&mut String::new())?;
    Ok(())
}
