//! The chats of a project: continue one, start a new one, read or delete one
use super::*;

impl App {
    pub(super) fn load_chats(
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

    pub(super) fn chats(
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
}
