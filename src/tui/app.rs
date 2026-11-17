use clap::ValueEnum;
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
use crate::project::list::Row;
use crate::snapshot::{Snapshot, review};
use crate::{project, snapshot};

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
                    (Ok(chats), ChatPurpose::Open) if chats.list.len() < 2 => {
                        self.pending = open(Session::Chat(project, Start::Last));
                    }
                    (Ok(chats), ChatPurpose::Remove) if chats.list.is_empty() => {
                        self.error = Some(format!("{project} has no chats yet"));
                    }
                    (Ok(chats), purpose) => {
                        self.mode = Mode::Chats {
                            project,
                            agent,
                            purpose,
                            chats,
                            selected: 0,
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
            General::Upgrade => run(Task::Upgrade),
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
            ProjectItem::NewChat if project.busy => {
                self.error = Some(format!(
                    "the agent in {name} is busy with a task, new chat is available when it finishes"
                ));
                None
            }
            ProjectItem::NewChat => open(Session::Chat(name, Start::New)),
            ProjectItem::RemoveChat => {
                self.load_chats(name, agent_of(project), ChatPurpose::Remove)
            }
            ProjectItem::Shell => open(Session::Shell(name)),
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
        let last = chats.list.len().saturating_sub(1);
        let selected = match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                let chat = chats.list.get(selected)?;
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
            KeyCode::Char('v') => {
                let chat = chats.list.get(selected)?;
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

    fn confirm(&mut self, question: String, action: Action) -> Option<Outcome> {
        self.mode = Mode::Confirm { question, action };
        None
    }
}

const VIEW_PAGE: usize = 20;
const VIEW_TAIL: usize = 30;
const BUSY_SWITCH: &str =
    "the agent is busy with a task in the current chat, switch when it finishes";
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
