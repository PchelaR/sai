use std::env;

use clap::ValueEnum;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::actions::{Action, Progress, Session, Task};
use super::loader::Loaded;
use super::menu::{General, ProjectItem};
use crate::agent::Agent;
use crate::project::list::Row;
use crate::snapshot;

pub enum Outcome {
    Quit,
    Refresh,
    Run(Action),
}

pub enum Input {
    Attach,
    RestorePath { path: String, snapshot: String },
    ResetConfirmation,
}

#[derive(Clone, Copy)]
pub enum AgentChoice {
    Attach,
    Change,
}

#[derive(Default)]
pub enum Mode {
    #[default]
    Browse,
    ProjectMenu {
        selected: usize,
    },
    Input {
        purpose: Input,
        value: String,
    },
    ChooseAgent {
        path: String,
        selected: usize,
        purpose: AgentChoice,
    },
    Snapshots {
        path: String,
        stamps: Vec<String>,
        selected: usize,
    },
    Confirm {
        question: String,
        action: Action,
    },
    Running {
        title: String,
        log: Vec<String>,
        result: Option<Result<(), String>>,
    },
}

#[derive(Default)]
pub struct App {
    pub loaded: Option<Loaded>,
    pub error: Option<String>,
    pub loading: bool,
    pub selected: usize,
    pub mode: Mode,
}

impl App {
    pub fn update(&mut self, loaded: Result<Loaded, String>) {
        self.loading = false;
        match loaded {
            Ok(loaded) => {
                self.loaded = Some(loaded);
                self.error = None;
                self.selected = self.selected.min(self.items() - 1);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn start(&mut self, title: String) {
        self.mode = Mode::Running {
            title,
            log: Vec::new(),
            result: None,
        };
    }

    pub fn progress(&mut self, update: Progress) {
        let Mode::Running { log, result, .. } = &mut self.mode else {
            return;
        };
        match update {
            Progress::Line(line) => log.push(line),
            Progress::Done(done) => *result = Some(done),
        }
    }

    pub fn projects(&self) -> &[Row] {
        self.loaded.as_ref().map_or(&[], |loaded| &loaded.projects)
    }

    pub fn selected_project(&self) -> Option<&Row> {
        self.projects().get(self.selected)
    }

    pub fn selected_general(&self) -> Option<usize> {
        self.selected.checked_sub(self.projects().len())
    }

    pub fn handle(&mut self, key: KeyEvent) -> Option<Outcome> {
        match std::mem::take(&mut self.mode) {
            Mode::Browse => self.browse(key),
            Mode::ProjectMenu { selected } => self.project_menu(selected, key),
            Mode::Input { purpose, value } => self.input(purpose, value, key),
            Mode::ChooseAgent {
                path,
                selected,
                purpose,
            } => self.choose_agent(path, selected, purpose, key),
            Mode::Snapshots {
                path,
                stamps,
                selected,
            } => self.snapshots(path, stamps, selected, key),
            Mode::Confirm { question, action } => match key.code {
                KeyCode::Char('y') => Some(Outcome::Run(action)),
                KeyCode::Char('n') | KeyCode::Esc => None,
                _ => {
                    self.mode = Mode::Confirm { question, action };
                    None
                }
            },
            Mode::Running { title, log, result } => {
                let finished = result.is_some();
                if finished && matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    return Some(Outcome::Refresh);
                }
                self.mode = Mode::Running { title, log, result };
                None
            }
        }
    }

    fn items(&self) -> usize {
        self.projects().len() + General::ALL.len()
    }

    fn browse(&mut self, key: KeyEvent) -> Option<Outcome> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.items() - 1);
                None
            }
            KeyCode::Enter => match self.selected_general() {
                Some(index) => self.general(General::ALL[index]),
                None => {
                    self.mode = Mode::ProjectMenu { selected: 0 };
                    None
                }
            },
            KeyCode::Esc => Some(Outcome::Quit),
            KeyCode::Char(character) => self.shortcut(character),
            _ => None,
        }
    }

    fn shortcut(&mut self, character: char) -> Option<Outcome> {
        if let Some(item) = General::ALL.iter().find(|item| item.key() == character) {
            return self.general(*item);
        }
        let item = ProjectItem::ALL
            .iter()
            .find(|item| item.key() == character)?;
        self.project_item(*item)
    }

    fn general(&mut self, item: General) -> Option<Outcome> {
        match item {
            General::Attach => self.ask(Input::Attach, current_dir()),
            General::Update => run(Task::Update),
            General::Doctor => run(Task::Doctor),
            General::Reset => self.ask(Input::ResetConfirmation, String::new()),
            General::Refresh => Some(Outcome::Refresh),
            General::Quit => Some(Outcome::Quit),
        }
    }

    fn project_menu(&mut self, selected: usize, key: KeyEvent) -> Option<Outcome> {
        let last = ProjectItem::ALL.len() - 1;
        let selected = match key.code {
            KeyCode::Enter => return self.project_item(ProjectItem::ALL[selected]),
            KeyCode::Esc => return None,
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(last),
            KeyCode::Char(character) => {
                if let Some(item) = ProjectItem::ALL.iter().find(|item| item.key() == character) {
                    return self.project_item(*item);
                }
                selected
            }
            _ => selected,
        };
        self.mode = Mode::ProjectMenu { selected };
        None
    }

    fn project_item(&mut self, item: ProjectItem) -> Option<Outcome> {
        let project = self.selected_project()?;
        let (name, path) = (project.name.clone(), project.path.clone());
        match item {
            ProjectItem::Chat => open(Session::Chat(path)),
            ProjectItem::Shell => open(Session::Shell(path)),
            ProjectItem::Stop => run(Task::Stop(name)),
            ProjectItem::Change => {
                self.mode = Mode::ChooseAgent {
                    path,
                    selected: 0,
                    purpose: AgentChoice::Change,
                };
                None
            }
            ProjectItem::Snapshots => {
                match snapshot::stamps(&path) {
                    Ok(stamps) => {
                        self.mode = Mode::Snapshots {
                            path,
                            stamps,
                            selected: 0,
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
                None
            }
            ProjectItem::Remove => self.confirm(
                format!("Remove {name}? Files and snapshots on the VPS stay"),
                Action::Run(Task::Remove { name, purge: false }),
            ),
            ProjectItem::Purge => self.confirm(
                format!("Remove {name} with its files and snapshots on the VPS?"),
                Action::Run(Task::Remove { name, purge: true }),
            ),
        }
    }

    fn input(&mut self, purpose: Input, mut value: String, key: KeyEvent) -> Option<Outcome> {
        match key.code {
            KeyCode::Enter => return self.submit(purpose, value.trim().to_owned()),
            KeyCode::Esc => return None,
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(character) => value.push(character),
            _ => {}
        }
        self.mode = Mode::Input { purpose, value };
        None
    }

    fn submit(&mut self, purpose: Input, value: String) -> Option<Outcome> {
        match purpose {
            Input::Attach => {
                self.mode = Mode::ChooseAgent {
                    path: expand_home(&value),
                    selected: 0,
                    purpose: AgentChoice::Attach,
                };
                None
            }
            Input::RestorePath { path, snapshot } => {
                let target = (!value.is_empty()).then_some(value);
                let what = target.as_deref().unwrap_or("the whole project").to_owned();
                self.confirm(
                    format!("Restore {what} from {snapshot}? Current files are snapshotted first"),
                    Action::Run(Task::Restore {
                        path,
                        snapshot,
                        target,
                    }),
                )
            }
            Input::ResetConfirmation => {
                let ip = self.loaded.as_ref().map(|loaded| loaded.ip.as_str());
                if ip == Some(value.as_str()) {
                    return run(Task::Reset);
                }
                self.error =
                    Some("reset confirmation did not match, nothing was deleted".to_owned());
                None
            }
        }
    }

    fn choose_agent(
        &mut self,
        path: String,
        selected: usize,
        purpose: AgentChoice,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let agents = Agent::value_variants();
        let selected = match key.code {
            KeyCode::Enter => {
                let agent = agents[selected];
                return run(match purpose {
                    AgentChoice::Attach => Task::Attach(path, agent),
                    AgentChoice::Change => Task::Change(path, agent),
                });
            }
            KeyCode::Esc => return None,
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(agents.len() - 1),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::ChooseAgent {
            path,
            selected,
            purpose,
        };
        None
    }

    fn snapshots(
        &mut self,
        path: String,
        stamps: Vec<String>,
        selected: usize,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let last = stamps.len().saturating_sub(1);
        let selected = match key.code {
            KeyCode::Enter => {
                let snapshot = stamps.get(selected)?.clone();
                return self.ask(Input::RestorePath { path, snapshot }, String::new());
            }
            KeyCode::Esc => return None,
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::Snapshots {
            path,
            stamps,
            selected,
        };
        None
    }

    fn ask(&mut self, purpose: Input, value: String) -> Option<Outcome> {
        self.mode = Mode::Input { purpose, value };
        None
    }

    fn confirm(&mut self, question: String, action: Action) -> Option<Outcome> {
        self.mode = Mode::Confirm { question, action };
        None
    }
}

fn run(task: Task) -> Option<Outcome> {
    Some(Outcome::Run(Action::Run(task)))
}

fn open(session: Session) -> Option<Outcome> {
    Some(Outcome::Run(Action::Open(session)))
}

fn current_dir() -> String {
    env::current_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default()
}

fn expand_home(input: &str) -> String {
    match (input.strip_prefix("~/"), etcetera::home_dir()) {
        (Some(relative), Ok(home)) => home.join(relative).display().to_string(),
        _ => input.to_owned(),
    }
}
