//! The list of projects, the general menu and the menu of a project
use super::*;

impl App {
    pub(super) fn items(&self) -> usize {
        self.projects().len() + General::ALL.len()
    }

    pub(super) fn browse(&mut self, key: KeyEvent) -> Option<Outcome> {
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

    pub(super) fn shortcut(&mut self, character: char) -> Option<Outcome> {
        if let Some(item) = General::ALL.iter().find(|item| item.key() == character) {
            return self.general(*item);
        }
        let item = ProjectItem::ALL
            .iter()
            .find(|item| item.key() == character)?;
        self.project_item(*item)
    }

    pub(super) fn general(&mut self, item: General) -> Option<Outcome> {
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

    pub(super) fn project_menu(&mut self, selected: usize, key: KeyEvent) -> Option<Outcome> {
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

    pub(super) fn project_item(&mut self, item: ProjectItem) -> Option<Outcome> {
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
}
