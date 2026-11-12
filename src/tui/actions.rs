use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::agent::Agent;
use crate::project::{Registry, change, detach, remove, rename, stop};
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
    Attach {
        project: String,
        path: String,
        agent: Option<Agent>,
        large: bool,
        git: bool,
    },
    Detach {
        project: String,
        path: String,
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
            Task::Attach {
                project,
                path,
                agent: Some(agent),
                ..
            } => format!("attach {project} {path} --agent {agent}"),
            Task::Attach { project, path, .. } => format!("attach {project} {path}"),
            Task::Detach { project, path } => format!("detach {project} {path}"),
            Task::Change(project, agent) => format!("change {agent} {project}"),
            Task::Rename { project, name } => format!("rename {project} {name}"),
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
        Session::Chat(project) => chat::run(Some(&project), None),
        Session::Shell(project) => shell::run(Some(&project)),
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
        Task::Attach {
            project,
            path,
            agent,
            large,
            git,
        } => attach::run(
            &project,
            &path,
            agent,
            None,
            &move |_, _| Ok(large),
            &move |_| Ok(git),
        ),
        Task::Detach { project, path } => detach::run(&project, &path),
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
