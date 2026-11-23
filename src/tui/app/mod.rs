use clap::ValueEnum;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent};

use super::actions::{Action, Progress, Session, Task};
use super::explorer::{Explorer, Step};
use super::loader::Loaded;
use super::menu::{General, ProjectItem};
use crate::agent::Agent;
use crate::agent::Start;
use crate::chat::chats::{self, Chats};
use crate::project::folders::Plan;
use crate::project::list::{self, Row};
use crate::snapshot::{Snapshot, review};
use crate::{project, snapshot, upgrade};

mod browse;
mod chat_list;
mod folders;
mod input;
mod snapshots;
#[cfg(test)]
mod tests;

pub enum Outcome {
    Quit,
    Refresh,
    Run(Action),
}

pub enum Input {
    Project,
    Rename { project: String },
    RestorePath { project: String, snapshot: String },
    ResetConfirmation,
    PurgeConfirmation { project: String },
}

pub enum ChatPurpose {
    Open,
    Remove,
}

impl ChatPurpose {
    /// Opening a chat lists "+ new chat" first, above the chats
    pub fn offers_new(&self) -> bool {
        matches!(self, ChatPurpose::Open)
    }
}

pub enum AgentChoice {
    Attach { project: String },
    Change { project: String },
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
        selected: usize,
        purpose: AgentChoice,
    },
    LoadingChats {
        project: String,
        agent: Agent,
        purpose: ChatPurpose,
        receiver: Receiver<Result<Chats, String>>,
    },
    Chats {
        project: String,
        agent: Agent,
        purpose: ChatPurpose,
        chats: Chats,
        selected: usize,
    },
    LoadingChatView {
        title: String,
        receiver: Receiver<Result<String, String>>,
        back: Box<Mode>,
    },
    ChatView {
        title: String,
        lines: Vec<String>,
        scroll: usize,
        back: Box<Mode>,
    },
    LoadingReview {
        project: String,
        receiver: Receiver<Result<Vec<String>, String>>,
    },
    Review {
        project: String,
        lines: Vec<String>,
    },
    LoadingSnapshots {
        project: String,
        receiver: Receiver<Result<Vec<Snapshot>, String>>,
    },
    Snapshots {
        project: String,
        snapshots: Vec<Snapshot>,
        selected: usize,
    },
    Explorer {
        explorer: Box<Explorer>,
        agent: Option<Agent>,
    },
    Apply {
        explorer: Box<Explorer>,
        agent: Option<Agent>,
        plan: Plan,
        question: String,
        asks_git: bool,
    },
    Confirm {
        question: String,
        action: Action,
    },
    Help,
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
    pub input_error: Option<String>,
    pub completions: Vec<String>,
    pub loading: bool,
    pub selected: usize,
    pub mode: Mode,
    pub pending: Option<Outcome>,
    /// The user switched to another window, so the menu calls them when an agent stops.
    /// Terminals that do not report focus keep this false: the menu stays quiet rather
    /// than beeping at someone who watches it
    pub unfocused: bool,
    agent_states: HashMap<String, String>,
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

    /// Takes fresh agent states and tells whether an agent now waits for the user
    pub fn agent_states(&mut self, states: HashMap<String, String>) -> bool {
        let call = states.iter().any(|(name, now)| {
            self.agent_states
                .get(name)
                .is_some_and(|before| list::needs_user(before, now))
        });
        if let Some(loaded) = &mut self.loaded {
            for row in loaded.projects.iter_mut().filter(|row| row.live()) {
                if let Some(state) = states.get(&row.name) {
                    row.set_agent_state(state);
                }
            }
        }
        self.agent_states = states;
        call
    }

    /// Inside a chat the agent rings the bell itself, so the menu starts over afterwards
    /// instead of ringing again for what happened during the chat
    pub fn forget_agent_states(&mut self) {
        self.agent_states.clear();
        self.unfocused = false;
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

    pub fn mouse(&mut self, event: MouseEvent) {
        if let Mode::Explorer { explorer, .. } = &mut self.mode {
            explorer.mouse(event);
        }
    }

    pub fn tick(&mut self) -> bool {
        match &mut self.mode {
            Mode::Explorer { explorer, .. } => explorer.tick(),
            Mode::LoadingChats {
                project,
                agent,
                purpose,
                receiver,
            } => {
                let Ok(result) = receiver.try_recv() else {
                    return false;
                };
                let project = std::mem::take(project);
                let agent = *agent;
                let purpose = std::mem::replace(purpose, ChatPurpose::Open);
                self.mode = Mode::Browse;
                match (result, purpose) {
                    (Err(error), _) => self.error = Some(error),
                    // Nothing to continue: straight into a new chat
                    (Ok(chats), ChatPurpose::Open) if chats.list.is_empty() => {
                        self.pending = open(Session::Chat(project, Start::Last));
                    }
                    (Ok(chats), ChatPurpose::Remove) if chats.list.is_empty() => {
                        self.error = Some(format!("{project} has no chats yet"));
                    }
                    // On the latest chat, so Enter continues it as before
                    (Ok(chats), purpose) => {
                        self.mode = Mode::Chats {
                            project,
                            agent,
                            selected: usize::from(purpose.offers_new()),
                            purpose,
                            chats,
                        }
                    }
                }
                true
            }
            Mode::LoadingChatView { receiver, .. } => {
                let Ok(result) = receiver.try_recv() else {
                    return false;
                };
                let Mode::LoadingChatView { title, back, .. } = std::mem::take(&mut self.mode)
                else {
                    return false;
                };
                match result {
                    Ok(text) => {
                        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                        self.mode = Mode::ChatView {
                            title,
                            scroll: lines.len().saturating_sub(VIEW_TAIL),
                            lines,
                            back,
                        };
                    }
                    Err(error) => {
                        self.mode = *back;
                        self.error = Some(error);
                    }
                }
                true
            }
            Mode::LoadingReview { project, receiver } => {
                let Ok(result) = receiver.try_recv() else {
                    return false;
                };
                let project = std::mem::take(project);
                match result {
                    Ok(lines) if lines.is_empty() => self.mode = Mode::ProjectMenu { selected: 0 },
                    Ok(lines) => self.mode = Mode::Review { project, lines },
                    Err(error) => {
                        self.mode = Mode::ProjectMenu { selected: 0 };
                        self.error = Some(error);
                    }
                }
                true
            }
            Mode::LoadingSnapshots { project, receiver } => {
                let Ok(result) = receiver.try_recv() else {
                    return false;
                };
                let project = std::mem::take(project);
                match result {
                    Ok(snapshots) => {
                        self.mode = Mode::Snapshots {
                            project,
                            snapshots,
                            selected: 0,
                        }
                    }
                    Err(error) => {
                        self.mode = Mode::Browse;
                        self.error = Some(error);
                    }
                }
                true
            }
            _ => false,
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
            Mode::ChooseAgent { selected, purpose } => self.choose_agent(selected, purpose, key),
            Mode::LoadingChats {
                project,
                agent,
                purpose,
                receiver,
            } => {
                if key.code != KeyCode::Esc {
                    self.mode = Mode::LoadingChats {
                        project,
                        agent,
                        purpose,
                        receiver,
                    };
                }
                None
            }
            Mode::Chats {
                project,
                agent,
                purpose,
                chats,
                selected,
            } => self.chats(project, agent, purpose, chats, selected, key),
            Mode::LoadingChatView {
                title,
                receiver,
                back,
            } => {
                self.mode = if key.code == KeyCode::Esc {
                    *back
                } else {
                    Mode::LoadingChatView {
                        title,
                        receiver,
                        back,
                    }
                };
                None
            }
            Mode::ChatView {
                title,
                lines,
                scroll,
                back,
            } => {
                let last = lines.len().saturating_sub(1);
                let scroll = match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.mode = *back;
                        return None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => (scroll + 1).min(last),
                    KeyCode::PageUp => scroll.saturating_sub(VIEW_PAGE),
                    KeyCode::PageDown => (scroll + VIEW_PAGE).min(last),
                    KeyCode::Home => 0,
                    KeyCode::End => lines.len().saturating_sub(VIEW_TAIL),
                    _ => scroll,
                };
                self.mode = Mode::ChatView {
                    title,
                    lines,
                    scroll,
                    back,
                };
                None
            }
            Mode::LoadingReview { project, receiver } => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::ProjectMenu { selected: 0 };
                } else {
                    self.mode = Mode::LoadingReview { project, receiver };
                }
                None
            }
            Mode::Review { project, lines } => {
                match key.code {
                    KeyCode::Enter => {
                        self.mark_reviewed(&project);
                        self.mode = Mode::ProjectMenu { selected: 0 };
                    }
                    KeyCode::Esc => self.mode = Mode::ProjectMenu { selected: 0 },
                    _ => self.mode = Mode::Review { project, lines },
                }
                None
            }
            Mode::LoadingSnapshots { project, receiver } => {
                if key.code != KeyCode::Esc {
                    self.mode = Mode::LoadingSnapshots { project, receiver };
                }
                None
            }
            Mode::Snapshots {
                project,
                snapshots,
                selected,
            } => self.snapshots(project, snapshots, selected, key),
            Mode::Explorer { explorer, agent } => self.explore(explorer, agent, key),
            Mode::Apply {
                explorer,
                agent,
                plan,
                question,
                asks_git,
            } => self.apply(explorer, agent, plan, question, asks_git, key),
            Mode::Confirm { question, action } => match key.code {
                KeyCode::Char('y') => Some(Outcome::Run(action)),
                KeyCode::Char('n') | KeyCode::Esc => None,
                _ => {
                    self.mode = Mode::Confirm { question, action };
                    None
                }
            },
            Mode::Help => None,
            Mode::Running { title, log, result } => {
                let finished = result.is_some();
                if finished && matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    // Declined, the header keeps reminding until sai restarts
                    if let Some(version) = upgrade::installed()
                        && title == Task::Upgrade.title()
                    {
                        return self.offer_restart(version);
                    }
                    return Some(Outcome::Refresh);
                }
                self.mode = Mode::Running { title, log, result };
                None
            }
        }
    }

    fn offer_restart(&mut self, version: &str) -> Option<Outcome> {
        self.confirm(
            format!("sai {version} is installed. Restart sai now to use it?"),
            Action::Restart,
        )
    }

    fn confirm(&mut self, question: String, action: Action) -> Option<Outcome> {
        self.mode = Mode::Confirm { question, action };
        None
    }
}

const VIEW_PAGE: usize = 20;
const VIEW_TAIL: usize = 30;
const BUSY_SWITCH: &str =
    "the agent is busy with a task in the current chat, switch when it finishes";
const BUSY_NEW: &str = "the agent is busy with a task, start a new chat when it finishes";
const BUSY_REMOVE: &str = "the agent is busy in this chat, delete it when the agent finishes";

fn agent_of(project: &Row) -> Agent {
    Agent::parse(&project.agent).unwrap_or_default()
}

fn run(task: Task) -> Option<Outcome> {
    Some(Outcome::Run(Action::Run(task)))
}

fn open(session: Session) -> Option<Outcome> {
    Some(Outcome::Run(Action::Open(session)))
}

pub fn expand_home(input: &str) -> String {
    let relative = input
        .strip_prefix("~/")
        .or_else(|| input.strip_prefix("~\\"));
    match (relative, etcetera::home_dir()) {
        (Some(relative), Ok(home)) => home.join(relative).display().to_string(),
        _ => input.to_owned(),
    }
}
