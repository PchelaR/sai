//! Typed answers: names, paths and confirmations, and the choice of an agent
use super::*;

impl App {
    pub(super) fn input(
        &mut self,
        purpose: Input,
        mut value: String,
        key: KeyEvent,
    ) -> Option<Outcome> {
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
                        crate::tui::complete::word(&value, &names)
                    }
                    _ => crate::tui::complete::Completion {
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

    pub(super) fn submit(&mut self, purpose: Input, value: String) -> Option<Outcome> {
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

    pub(super) fn choose_agent(
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

    pub(super) fn suggested_project(&self) -> String {
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

    pub(super) fn ask(&mut self, purpose: Input, value: String) -> Option<Outcome> {
        self.input_error = None;
        self.mode = Mode::Input { purpose, value };
        None
    }

    pub(super) fn ask_again(
        &mut self,
        purpose: Input,
        value: String,
        error: anyhow::Error,
    ) -> Option<Outcome> {
        self.mode = Mode::Input { purpose, value };
        self.input_error = Some(format!("{error:#}"));
        None
    }
}
