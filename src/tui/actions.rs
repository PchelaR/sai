use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::agent::Agent;
use crate::project::{change, remove, stop};
use crate::{attach, chat, doctor, logs, output, reset, shell, snapshot, update};

pub enum Action {
    Open(Session),
    Run(Task),
}

pub enum Session {
    Chat(String),
    Shell(String),
}

pub enum Task {
    Attach(String, Agent),
    Change(String, Agent),
    Restore {
        path: String,
        snapshot: String,
        target: Option<String>,
    },
    Stop(String),
    Remove {
        name: String,
        purge: bool,
    },
    Update,
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
            Task::Attach(path, agent) => format!("attach {path} --agent {agent}"),
            Task::Change(path, agent) => format!("change {agent} {path}"),
            Task::Restore { snapshot, .. } => format!("restore {snapshot}"),
            Task::Stop(name) => format!("stop {name}"),
            Task::Remove { name, purge: false } => format!("rm {name}"),
            Task::Remove { name, purge: true } => format!("rm {name} --purge"),
            Task::Update => "update".to_owned(),
            Task::Doctor => "doctor".to_owned(),
            Task::Logs => "logs".to_owned(),
            Task::Reset => "reset".to_owned(),
        }
    }
}

pub fn open(terminal: &mut DefaultTerminal, session: Session) -> Result<()> {
    ratatui::restore();
    let result = match session {
        Session::Chat(path) => chat::run(&path, None),
        Session::Shell(path) => shell::run(&path),
    };
    if let Err(error) = &result {
        logs::error(error);
        eprintln!("Error: {error:#}");
        wait_for_enter()?;
    }
    *terminal = ratatui::init();
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
        Task::Attach(path, agent) => attach::run(&path, Some(agent)),
        Task::Change(path, agent) => change::run(&path, agent),
        Task::Restore {
            path,
            snapshot,
            target,
        } => snapshot::restore(&path, &snapshot, target.as_deref()),
        Task::Stop(name) => stop::run(&name),
        Task::Remove { name, purge } => remove::run(&name, purge),
        Task::Update => update::run(),
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
