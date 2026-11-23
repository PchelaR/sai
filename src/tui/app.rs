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
                None if self.selected_project()?.only_on_vps => {
                    self.project_item(ProjectItem::Folders)
                }
                None => {
                    let project = self.selected_project()?;
                    if project.review > 0 {
                        self.load_review(project.name.clone());
                    } else {
                        self.mode = Mode::ProjectMenu { selected: 0 };
                    }
                    None
                }
            },
            KeyCode::Esc => Some(Outcome::Quit),
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                None
            }
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
            General::Attach => {
                let suggested = self.suggested_project();
                self.ask(Input::Project, suggested)
            }
            General::Vps => open(Session::Vps),
            General::Update => run(Task::Update),
            General::Upgrade => match upgrade::installed() {
                Some(version) => self.offer_restart(version),
                None => run(Task::Upgrade),
            },
            General::Doctor => run(Task::Doctor),
            General::Logs => run(Task::Logs),
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
        let name = project.name.clone();
        if project.only_on_vps && !matches!(item, ProjectItem::Folders | ProjectItem::Purge) {
            self.error = Some(format!(
                "{name} is only on the VPS. Press Enter to attach a folder to it, or D to delete its files"
            ));
            return None;
        }
        match item {
            ProjectItem::Chat => self.load_chats(name, agent_of(project), ChatPurpose::Open),
            ProjectItem::RemoveChat => {
                self.load_chats(name, agent_of(project), ChatPurpose::Remove)
            }
            ProjectItem::Shell => open(Session::Shell(name)),
            ProjectItem::History => open(Session::History(name)),
            ProjectItem::Services => open(Session::Services(name)),
            ProjectItem::Stop => run(Task::Stop(name)),
            ProjectItem::Folders if project.only_on_vps => {
                self.mode = Mode::ChooseAgent {
                    selected: 0,
                    purpose: AgentChoice::Attach { project: name },
                };
                None
            }
            ProjectItem::Folders => self.explorer(name, None),
            ProjectItem::Rename => {
                let value = name.clone();
                self.ask(Input::Rename { project: name }, value)
            }
            ProjectItem::Change => {
                self.mode = Mode::ChooseAgent {
                    selected: 0,
                    purpose: AgentChoice::Change { project: name },
                };
                None
            }
            ProjectItem::Snapshots => {
                let (sender, receiver) = mpsc::channel();
                let project = name.clone();
                thread::spawn(move || {
                    sender
                        .send(snapshot::snapshots(&project).map_err(|error| format!("{error:#}")))
                        .ok();
                });
                self.mode = Mode::LoadingSnapshots {
                    project: name,
                    receiver,
                };
                None
            }
            ProjectItem::Remove => self.confirm(
                format!("Remove {name}? Files and snapshots on the VPS stay"),
                Action::Run(Task::Remove { name, purge: false }),
            ),
            ProjectItem::Purge => {
                self.ask(Input::PurgeConfirmation { project: name }, String::new())
            }
        }
    }

    fn input(&mut self, purpose: Input, mut value: String, key: KeyEvent) -> Option<Outcome> {
        self.completions.clear();
        match key.code {
            KeyCode::Tab => {
                let completion = match &purpose {
                    Input::Project => {
                        let names: Vec<&str> = self
                            .projects()
                            .iter()
                            .map(|row| row.name.as_str())
                            .collect();
                        super::complete::word(&value, &names)
                    }
                    _ => super::complete::Completion {
                        value: value.clone(),
                        options: Vec::new(),
                    },
                };
                value = completion.value;
                self.completions = completion.options;
            }
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
            Input::Project => {
                let value = match project::name_from(&value) {
                    Ok(name) => name,
                    Err(error) => return self.ask_again(Input::Project, value, error),
                };
                if self
                    .projects()
                    .iter()
                    .any(|row| row.name == value && !row.only_on_vps)
                {
                    return self.explorer(value, None);
                }
                self.mode = Mode::ChooseAgent {
                    selected: 0,
                    purpose: AgentChoice::Attach { project: value },
                };
                None
            }
            Input::Rename { project } => {
                let value = match project::name_from(&value) {
                    Ok(name) => name,
                    Err(error) => return self.ask_again(Input::Rename { project }, value, error),
                };
                if value == project {
                    return None;
                }
                run(Task::Rename {
                    project,
                    name: value,
                })
            }
            Input::RestorePath { project, snapshot } => {
                let target = (!value.is_empty()).then_some(value);
                let what = target.as_deref().unwrap_or("the whole project").to_owned();
                self.confirm(
                    format!("Restore {what} from {snapshot}? Current files are snapshotted first"),
                    Action::Run(Task::Restore {
                        project,
                        snapshot,
                        target,
                    }),
                )
            }
            Input::PurgeConfirmation { project } => {
                if value == project {
                    return run(Task::Remove {
                        name: project,
                        purge: true,
                    });
                }
                self.error = Some("the name did not match, nothing was deleted".to_owned());
                None
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
        selected: usize,
        purpose: AgentChoice,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let agents = Agent::value_variants();
        let selected = match key.code {
            KeyCode::Enter => {
                let agent = agents[selected];
                return match purpose {
                    AgentChoice::Attach { project } => self.explorer(project, Some(agent)),
                    AgentChoice::Change { project } => run(Task::Change(project, agent)),
                };
            }
            KeyCode::Esc => return None,
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(agents.len() - 1),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::ChooseAgent { selected, purpose };
        None
    }

    fn snapshots(
        &mut self,
        project: String,
        snapshots: Vec<Snapshot>,
        selected: usize,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let last = snapshots.len().saturating_sub(1);
        let selected = match key.code {
            KeyCode::Enter => {
                let snapshot = snapshots.get(selected)?.stamp.clone();
                return self.ask(Input::RestorePath { project, snapshot }, String::new());
            }
            KeyCode::Esc => return None,
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::Snapshots {
            project,
            snapshots,
            selected,
        };
        None
    }

    fn load_chats(
        &mut self,
        project: String,
        agent: Agent,
        purpose: ChatPurpose,
    ) -> Option<Outcome> {
        let (sender, receiver) = mpsc::channel();
        let name = project.clone();
        thread::spawn(move || {
            sender
                .send(chats::load(&name, agent).map_err(|error| format!("{error:#}")))
                .ok();
        });
        self.mode = Mode::LoadingChats {
            project,
            agent,
            purpose,
            receiver,
        };
        None
    }

    fn chats(
        &mut self,
        project: String,
        agent: Agent,
        purpose: ChatPurpose,
        chats: Chats,
        selected: usize,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let offset = usize::from(purpose.offers_new());
        let last = (chats.list.len() + offset).saturating_sub(1);
        let new_chat = |app: &mut App, project: String| {
            if chats.busy() {
                app.error = Some(BUSY_NEW.to_owned());
                return None;
            }
            open(Session::Chat(project, Start::New))
        };
        let selected = match key.code {
            KeyCode::Esc => return None,
            KeyCode::Char('n') if purpose.offers_new() => return new_chat(self, project),
            KeyCode::Enter if purpose.offers_new() && selected == 0 => {
                return new_chat(self, project);
            }
            KeyCode::Enter => {
                let chat = chats.list.get(selected - offset)?;
                return match purpose {
                    ChatPurpose::Open => {
                        let switching =
                            chats.running().is_some_and(|running| running.id != chat.id);
                        if switching && chats.busy() {
                            self.error = Some(BUSY_SWITCH.to_owned());
                            return None;
                        }
                        open(Session::Chat(project, Start::Open(chat.id.clone())))
                    }
                    ChatPurpose::Remove => {
                        let current = chats.running().is_some_and(|running| running.id == chat.id);
                        if current && chats.busy() {
                            self.error = Some(BUSY_REMOVE.to_owned());
                            return None;
                        }
                        self.confirm(
                            format!("Delete the chat \"{}\"? This cannot be undone", chat.title),
                            Action::Run(Task::RemoveChat {
                                project,
                                agent,
                                chat: chat.id.clone(),
                            }),
                        )
                    }
                };
            }
            // Not on "+ new chat": it has nothing to read
            KeyCode::Char('v') if selected >= offset => {
                let chat = chats.list.get(selected - offset)?;
                let (sender, receiver) = mpsc::channel();
                let (name, id) = (project.clone(), chat.id.clone());
                thread::spawn(move || {
                    sender
                        .send(chats::show(&name, agent, &id).map_err(|error| format!("{error:#}")))
                        .ok();
                });
                let title = chat.title.clone();
                self.mode = Mode::LoadingChatView {
                    title,
                    receiver,
                    back: Box::new(Mode::Chats {
                        project,
                        agent,
                        purpose,
                        chats,
                        selected,
                    }),
                };
                return None;
            }
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::Chats {
            project,
            agent,
            purpose,
            chats,
            selected,
        };
        None
    }

    fn load_review(&mut self, project: String) {
        let (sender, receiver) = mpsc::channel();
        let name = project.clone();
        thread::spawn(move || {
            let result = review::unreviewed(&name)
                .map(|changes| review::lines(&changes))
                .map_err(|error| format!("{error:#}"));
            sender.send(result).ok();
        });
        self.mode = Mode::LoadingReview { project, receiver };
    }

    fn mark_reviewed(&mut self, project: &str) {
        let name = project.to_owned();
        thread::spawn(move || {
            review::mark(&name).ok();
        });
        if let Some(loaded) = &mut self.loaded
            && let Some(row) = loaded.projects.iter_mut().find(|row| row.name == project)
        {
            row.review = 0;
        }
    }

    fn explorer(&mut self, project: String, agent: Option<Agent>) -> Option<Outcome> {
        self.mode = Mode::Explorer {
            explorer: Box::new(Explorer::open(&project)),
            agent,
        };
        None
    }

    fn explore(
        &mut self,
        mut explorer: Box<Explorer>,
        agent: Option<Agent>,
        key: KeyEvent,
    ) -> Option<Outcome> {
        match explorer.key(key.code) {
            Step::Cancel => None,
            Step::Continue => {
                self.mode = Mode::Explorer { explorer, agent };
                None
            }
            Step::Apply(plan) if plan.is_empty() => None,
            Step::Apply(plan) => {
                let repositories = explorer.new_repositories(&plan);
                let asks_git = !repositories.is_empty();
                let mut question = plan.summary();
                if asks_git {
                    question.push(' ');
                    question.push_str(&crate::attach::git_question_many(&repositories));
                } else {
                    question.push_str(" Apply?");
                }
                self.mode = Mode::Apply {
                    explorer,
                    agent,
                    plan,
                    question,
                    asks_git,
                };
                None
            }
        }
    }

    fn apply(
        &mut self,
        explorer: Box<Explorer>,
        agent: Option<Agent>,
        plan: Plan,
        question: String,
        asks_git: bool,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let git = match (key.code, asks_git) {
            (KeyCode::Char('y'), _) => asks_git,
            (KeyCode::Char('n'), true) => false,
            (KeyCode::Char('n') | KeyCode::Esc, _) => {
                self.mode = Mode::Explorer { explorer, agent };
                return None;
            }
            _ => {
                self.mode = Mode::Apply {
                    explorer,
                    agent,
                    plan,
                    question,
                    asks_git,
                };
                return None;
            }
        };
        run(Task::Folders {
            project: explorer.project.clone(),
            agent,
            plan,
            git,
        })
    }

    fn suggested_project(&self) -> String {
        let Ok(current) = project::current_dir() else {
            return String::new();
        };
        self.projects()
            .iter()
            .find(|row| {
                row.folders
                    .iter()
                    .any(|folder| project::same_path(std::path::Path::new(&folder.path), &current))
            })
            .map(|row| row.name.clone())
            .or_else(|| project::folder_name(&current).ok())
            .unwrap_or_default()
    }

    fn ask(&mut self, purpose: Input, value: String) -> Option<Outcome> {
        self.input_error = None;
        self.mode = Mode::Input { purpose, value };
        None
    }

    fn ask_again(
        &mut self,
        purpose: Input,
        value: String,
        error: anyhow::Error,
    ) -> Option<Outcome> {
        self.mode = Mode::Input { purpose, value };
        self.input_error = Some(format!("{error:#}"));
        None
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

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    use std::collections::HashMap;
    use std::sync::mpsc;

    use clap::ValueEnum;

    use super::{
        Action, AgentChoice, App, ChatPurpose, Input, Loaded, Mode, Outcome, Progress, Session,
        Task,
    };
    use crate::agent::{Agent, Start};
    use crate::chat::chats::{Chat, Chats};
    use crate::project::list::Row;
    use crate::snapshot::Snapshot;
    use crate::status::Vps;
    use crate::upgrade;

    const IP: &str = "203.0.113.7";

    fn row(name: &str, only_on_vps: bool) -> Row {
        Row {
            only_on_vps,
            name: name.to_owned(),
            agent: "claude".to_owned(),
            state: "idle".to_owned(),
            sync: String::new(),
            review: 0,
            folders: Vec::new(),
        }
    }

    fn loaded(rows: Vec<Row>) -> Result<Loaded, String> {
        Ok(Loaded {
            ip: IP.to_owned(),
            projects: rows,
            vps: Vps::default(),
            latest: None,
        })
    }

    /// The menu with `shop`, `store` and `remote`, a project only on the VPS
    fn menu() -> App {
        let mut app = App::default();
        app.update(loaded(vec![
            row("shop", false),
            row("store", false),
            row("remote", true),
        ]));
        app
    }

    fn typing(app: &mut App, text: &str) {
        for character in text.chars() {
            assert!(press(app, KeyCode::Char(character)).is_none());
        }
    }

    fn erase(app: &mut App) {
        for _ in 0..40 {
            press(app, KeyCode::Backspace);
        }
    }

    fn runs(outcome: Option<Outcome>) -> Option<Task> {
        match outcome {
            Some(Outcome::Run(Action::Run(task))) => Some(task),
            _ => None,
        }
    }

    fn opened(outcome: Option<Outcome>) -> Option<Session> {
        match outcome {
            Some(Outcome::Run(Action::Open(session))) => Some(session),
            _ => None,
        }
    }

    #[test]
    fn moves_through_projects_and_the_general_menu() {
        let mut app = menu();
        assert!(press(&mut app, KeyCode::Up).is_none());
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_project().unwrap().name, "shop");
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected_project().unwrap().name, "store");
        for _ in 0..50 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected_general(), Some(super::General::ALL.len() - 1));
        assert!(app.selected_project().is_none());
        assert!(matches!(
            press(&mut app, KeyCode::Enter),
            Some(Outcome::Quit)
        ));

        // Fewer projects after a reload: the cursor stays inside the list
        app.update(loaded(vec![row("shop", false)]));
        assert_eq!(app.selected, super::General::ALL.len());
        // A failed reload keeps what was shown
        app.update(Err("cannot reach the VPS".to_owned()));
        assert_eq!(app.projects().len(), 1);
        assert!(app.error.is_some());
        app.update(loaded(vec![row("shop", false)]));
        assert!(app.error.is_none());

        assert!(press(&mut app, KeyCode::Char('?')).is_none());
        assert!(matches!(app.mode, Mode::Help));
        assert!(press(&mut app, KeyCode::Char('x')).is_none());
        assert!(matches!(app.mode, Mode::Browse), "any key leaves the help");
        assert!(matches!(press(&mut app, KeyCode::Esc), Some(Outcome::Quit)));
    }

    #[test]
    fn general_keys_work_from_anywhere_in_the_list() {
        let mut app = menu();
        assert!(matches!(
            press(&mut app, KeyCode::Char('q')),
            Some(Outcome::Quit)
        ));
        assert!(matches!(
            press(&mut app, KeyCode::Char('r')),
            Some(Outcome::Refresh)
        ));
        assert!(matches!(
            opened(press(&mut app, KeyCode::Char('v'))),
            Some(Session::Vps)
        ));
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('u'))),
            Some(Task::Update)
        ));
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('g'))),
            Some(Task::Doctor)
        ));
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('l'))),
            Some(Task::Logs)
        ));
        assert!(
            press(&mut app, KeyCode::Char('Z')).is_none(),
            "unknown keys do nothing"
        );
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn project_menu_and_its_shortcuts_act_on_the_selected_project() {
        let mut app = menu();
        press(&mut app, KeyCode::Down);
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(matches!(app.mode, Mode::ProjectMenu { selected: 0 }));
        press(&mut app, KeyCode::Down);
        assert!(matches!(app.mode, Mode::ProjectMenu { selected: 1 }));
        assert!(matches!(
            opened(press(&mut app, KeyCode::Char('s'))),
            Some(Session::Shell(name)) if name == "store"
        ));
        // The same keys straight from the list
        assert!(matches!(
            opened(press(&mut app, KeyCode::Char('e'))),
            Some(Session::Services(name)) if name == "store"
        ));
        assert!(matches!(
            opened(press(&mut app, KeyCode::Char('h'))),
            Some(Session::History(name)) if name == "store"
        ));
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('x'))),
            Some(Task::Stop(name)) if name == "store"
        ));
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(press(&mut app, KeyCode::Esc).is_none());
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn a_project_only_on_the_vps_offers_attaching_or_deleting() {
        let mut app = menu();
        app.selected = 2;
        assert!(press(&mut app, KeyCode::Char('s')).is_none());
        assert!(app.error.as_deref().unwrap().contains("only on the VPS"));
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(matches!(
            &app.mode,
            Mode::ChooseAgent { purpose: AgentChoice::Attach { project }, .. } if project == "remote"
        ));
        press(&mut app, KeyCode::Esc);
        assert!(press(&mut app, KeyCode::Char('D')).is_none());
        assert!(matches!(
            &app.mode,
            Mode::Input { purpose: Input::PurgeConfirmation { project }, .. } if project == "remote"
        ));
    }

    #[test]
    fn deleting_asks_first_and_purging_wants_the_name() {
        let mut app = menu();
        assert!(press(&mut app, KeyCode::Char('d')).is_none());
        assert!(press(&mut app, KeyCode::Char('n')).is_none());
        assert!(matches!(app.mode, Mode::Browse), "declined");
        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('y'))),
            Some(Task::Remove { name, purge: false }) if name == "shop"
        ));

        press(&mut app, KeyCode::Char('D'));
        typing(&mut app, "sho");
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(app.error.as_deref().unwrap().contains("did not match"));
        press(&mut app, KeyCode::Char('D'));
        typing(&mut app, "shop");
        assert!(matches!(
            runs(press(&mut app, KeyCode::Enter)),
            Some(Task::Remove { name, purge: true }) if name == "shop"
        ));
    }

    #[test]
    fn reset_wants_the_address_of_the_vps() {
        let mut app = menu();
        press(&mut app, KeyCode::Char('R'));
        typing(&mut app, "shop");
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(app.error.as_deref().unwrap().contains("did not match"));
        press(&mut app, KeyCode::Char('R'));
        typing(&mut app, IP);
        assert!(matches!(
            runs(press(&mut app, KeyCode::Enter)),
            Some(Task::Reset)
        ));
    }

    #[test]
    fn renaming_makes_a_valid_name_and_skips_the_same_one() {
        let mut app = menu();
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(&app.mode, Mode::Input { value, .. } if value == "shop"));
        assert!(press(&mut app, KeyCode::Enter).is_none(), "the same name");
        assert!(matches!(app.mode, Mode::Browse));

        press(&mut app, KeyCode::Char('n'));
        erase(&mut app);
        typing(&mut app, "!!!");
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(matches!(app.mode, Mode::Input { .. }), "asked again");
        assert!(app.input_error.is_some());
        erase(&mut app);
        typing(&mut app, "My Shop 2");
        assert!(matches!(
            runs(press(&mut app, KeyCode::Enter)),
            Some(Task::Rename { project, name }) if project == "shop" && name == "my-shop-2"
        ));
        press(&mut app, KeyCode::Char('n'));
        assert!(app.input_error.is_none(), "a new question starts clean");
        assert!(press(&mut app, KeyCode::Esc).is_none());
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn attaching_a_new_name_asks_for_the_agent() {
        let mut app = menu();
        app.mode = Mode::Input {
            purpose: Input::Project,
            value: "sh".to_owned(),
        };
        press(&mut app, KeyCode::Tab);
        assert!(matches!(&app.mode, Mode::Input { value, .. } if value == "shop"));
        erase(&mut app);
        typing(&mut app, "New Shop");
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(matches!(
            &app.mode,
            Mode::ChooseAgent { purpose: AgentChoice::Attach { project }, .. } if project == "new-shop"
        ));
        assert!(press(&mut app, KeyCode::Esc).is_none());
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn switching_the_agent_picks_from_the_list() {
        let mut app = menu();
        press(&mut app, KeyCode::Char('c'));
        press(&mut app, KeyCode::Up);
        for _ in 0..5 {
            press(&mut app, KeyCode::Down);
        }
        let agents = Agent::value_variants();
        assert!(matches!(
            runs(press(&mut app, KeyCode::Enter)),
            Some(Task::Change(project, agent)) if project == "shop" && agent == agents[agents.len() - 1]
        ));
    }

    #[test]
    fn restoring_a_snapshot_asks_what_and_confirms() {
        let mut app = menu();
        let snapshot = |stamp: &str| Snapshot {
            stamp: stamp.to_owned(),
            summary: String::new(),
        };
        app.mode = Mode::Snapshots {
            project: "shop".to_owned(),
            snapshots: vec![
                snapshot("2026-11-20_10-00-00"),
                snapshot("2026-11-19_10-00-00"),
            ],
            selected: 0,
        };
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(matches!(
            &app.mode,
            Mode::Input { purpose: Input::RestorePath { snapshot, .. }, .. } if snapshot == "2026-11-19_10-00-00"
        ));
        assert!(press(&mut app, KeyCode::Enter).is_none());
        assert!(
            matches!(&app.mode, Mode::Confirm { question, .. } if question.contains("the whole project"))
        );
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('y'))),
            Some(Task::Restore { project, target: None, .. }) if project == "shop"
        ));

        app.mode = Mode::Input {
            purpose: Input::RestorePath {
                project: "shop".to_owned(),
                snapshot: "2026-11-19_10-00-00".to_owned(),
            },
            value: String::new(),
        };
        typing(&mut app, "front/src");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(
            runs(press(&mut app, KeyCode::Char('y'))),
            Some(Task::Restore { target: Some(target), .. }) if target == "front/src"
        ));
    }

    #[test]
    fn a_task_shows_its_log_until_it_ends() {
        let mut app = menu();
        app.start(Task::Doctor.title());
        assert!(press(&mut app, KeyCode::Esc).is_none());
        assert!(
            matches!(app.mode, Mode::Running { .. }),
            "not while it runs"
        );
        app.progress(Progress::Line("ssh ok".to_owned()));
        app.progress(Progress::Done(Err("docker is missing".to_owned())));
        let Mode::Running { log, result, .. } = &app.mode else {
            panic!("still showing the task");
        };
        assert_eq!(log, &["ssh ok"]);
        assert!(result.as_ref().unwrap().is_err());
        assert!(matches!(
            press(&mut app, KeyCode::Esc),
            Some(Outcome::Refresh)
        ));
    }

    #[test]
    fn reading_a_chat_scrolls_and_goes_back_to_the_list() {
        let mut app = menu();
        app.mode = Mode::ChatView {
            title: "chat".to_owned(),
            lines: (0..100).map(|line| line.to_string()).collect(),
            scroll: 70,
            back: Box::new(Mode::ProjectMenu { selected: 3 }),
        };
        let scroll = |app: &App| match app.mode {
            Mode::ChatView { scroll, .. } => scroll,
            _ => panic!("left the chat"),
        };
        press(&mut app, KeyCode::Home);
        assert_eq!(scroll(&app), 0);
        press(&mut app, KeyCode::Up);
        assert_eq!(scroll(&app), 0);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(scroll(&app), 20);
        press(&mut app, KeyCode::End);
        assert_eq!(scroll(&app), 70);
        for _ in 0..50 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(scroll(&app), 99);
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::ProjectMenu { selected: 3 }));
    }

    #[test]
    fn loading_can_be_cancelled() {
        let mut app = menu();
        let (_sender, receiver) = mpsc::channel();
        app.mode = Mode::LoadingChats {
            project: "shop".to_owned(),
            agent: Agent::Claude,
            purpose: ChatPurpose::Open,
            receiver,
        };
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.mode, Mode::LoadingChats { .. }));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn opening_chats_follows_what_the_vps_has() {
        let mut app = menu();
        let load = |app: &mut App, purpose: ChatPurpose, count: usize| {
            let (sender, receiver) = mpsc::channel();
            app.mode = Mode::LoadingChats {
                project: "shop".to_owned(),
                agent: Agent::Claude,
                purpose,
                receiver,
            };
            assert!(!app.tick(), "nothing yet");
            let list = (0..count)
                .map(|i| Chat {
                    id: format!("c{i}"),
                    modified: 0,
                    title: format!("chat {i}"),
                })
                .collect();
            sender
                .send(Ok(Chats {
                    state: "idle".to_owned(),
                    clients: 0,
                    list,
                }))
                .unwrap();
            assert!(app.tick());
        };
        load(&mut app, ChatPurpose::Open, 0);
        assert!(
            matches!(
                opened(app.pending.take()),
                Some(Session::Chat(_, Start::Last))
            ),
            "no chats yet: straight into one"
        );
        load(&mut app, ChatPurpose::Open, 1);
        assert!(
            matches!(app.mode, Mode::Chats { selected: 1, .. }),
            "on the chat, below + new chat"
        );
        load(&mut app, ChatPurpose::Remove, 0);
        assert!(app.error.as_deref().unwrap().contains("no chats"));
        load(&mut app, ChatPurpose::Remove, 2);
        assert!(matches!(app.mode, Mode::Chats { selected: 0, .. }));
    }

    #[test]
    fn calls_the_user_when_an_agent_stops_or_asks() {
        let mut app = menu();
        let states = |state: &str| {
            HashMap::from([
                ("shop".to_owned(), state.to_owned()),
                ("remote".to_owned(), state.to_owned()),
            ])
        };
        assert!(!app.agent_states(states("working")), "nothing known before");
        assert!(app.agent_states(states("waiting")), "finished");
        assert_ne!(app.projects()[0].state, "idle", "the list shows it");
        assert_eq!(
            app.projects()[2].state,
            "idle",
            "a project only on the VPS has no agent here"
        );
        assert!(app.agent_states(states("asking")));
        assert!(!app.agent_states(states("asking")), "once per question");
        app.forget_agent_states();
        assert!(
            !app.agent_states(states("waiting")),
            "a chat in between starts over"
        );
    }

    fn chat_list(app: &mut App, purpose: ChatPurpose, state: &str) {
        let list = (0..2)
            .map(|i| Chat {
                id: format!("c{i}"),
                modified: 0,
                title: format!("chat {i}"),
            })
            .collect();
        let selected = usize::from(purpose.offers_new());
        app.mode = Mode::Chats {
            project: "shop".to_owned(),
            agent: Agent::Claude,
            purpose,
            chats: Chats {
                state: state.to_owned(),
                clients: 0,
                list,
            },
            selected,
        };
    }

    fn opens(outcome: Option<Outcome>) -> Option<Start> {
        match outcome {
            Some(Outcome::Run(Action::Open(Session::Chat(project, start)))) => {
                assert_eq!(project, "shop");
                Some(start)
            }
            _ => None,
        }
    }

    #[test]
    fn one_entry_continues_a_chat_or_starts_a_new_one() {
        let mut app = App::default();
        // The cursor starts on the latest chat: Enter continues it
        chat_list(&mut app, ChatPurpose::Open, "idle");
        let start = opens(press(&mut app, KeyCode::Enter));
        assert!(matches!(start, Some(Start::Open(id)) if id == "c0"));
        // "+ new chat" above it
        chat_list(&mut app, ChatPurpose::Open, "idle");
        assert!(press(&mut app, KeyCode::Up).is_none());
        assert!(press(&mut app, KeyCode::Char('v')).is_none());
        assert!(
            matches!(app.mode, Mode::Chats { selected: 0, .. }),
            "nothing to read there"
        );
        assert!(matches!(
            opens(press(&mut app, KeyCode::Enter)),
            Some(Start::New)
        ));
        // n from anywhere in the list
        chat_list(&mut app, ChatPurpose::Open, "idle");
        assert!(matches!(
            opens(press(&mut app, KeyCode::Char('n'))),
            Some(Start::New)
        ));
        // Not while the agent works in the current chat
        chat_list(&mut app, ChatPurpose::Open, "working");
        assert!(press(&mut app, KeyCode::Char('n')).is_none());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("busy"))
        );

        // Deleting lists only the chats
        chat_list(&mut app, ChatPurpose::Remove, "idle");
        assert!(press(&mut app, KeyCode::Char('n')).is_none());
        assert!(press(&mut app, KeyCode::Enter).is_none());
        let Mode::Confirm { question, .. } = &app.mode else {
            panic!("expected the question about deleting");
        };
        assert!(question.contains("chat 0"));
    }

    fn press(app: &mut App, code: KeyCode) -> Option<Outcome> {
        app.handle(KeyEvent::from(code))
    }

    fn finish(app: &mut App, task: Task) {
        app.start(task.title());
        app.progress(Progress::Line("Updated sai".to_owned()));
        app.progress(Progress::Done(Ok(())));
    }

    #[test]
    fn asks_to_restart_into_the_installed_sai_until_it_does() {
        let mut app = App::default();
        // Nothing installed yet: U downloads
        assert!(matches!(
            press(&mut app, KeyCode::Char('U')),
            Some(Outcome::Run(Action::Run(Task::Upgrade)))
        ));
        upgrade::pretend_installed("9.9.9");

        // Other tasks just go back to the list
        finish(&mut app, Task::Doctor);
        assert!(matches!(
            press(&mut app, KeyCode::Enter),
            Some(Outcome::Refresh)
        ));

        // After the upgrade: asked, not restarted without a yes
        finish(&mut app, Task::Upgrade);
        assert!(press(&mut app, KeyCode::Enter).is_none());
        let Mode::Confirm { question, .. } = &app.mode else {
            panic!("expected the question about restarting");
        };
        assert!(question.contains("9.9.9"));
        assert!(press(&mut app, KeyCode::Char('x')).is_none());
        assert!(
            matches!(app.mode, Mode::Confirm { .. }),
            "other keys keep the question"
        );
        assert!(press(&mut app, KeyCode::Char('n')).is_none());
        assert!(matches!(app.mode, Mode::Browse));

        // Declined: U asks again instead of downloading once more
        assert!(press(&mut app, KeyCode::Char('U')).is_none());
        assert!(matches!(app.mode, Mode::Confirm { .. }));
        assert!(matches!(
            press(&mut app, KeyCode::Char('y')),
            Some(Outcome::Run(Action::Restart))
        ));
    }
}
