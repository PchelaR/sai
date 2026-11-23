use ratatui::crossterm::event::{KeyCode, KeyEvent};

use std::collections::HashMap;
use std::sync::mpsc;

use clap::ValueEnum;

use super::{
    Action, AgentChoice, App, ChatPurpose, Input, Loaded, Mode, Outcome, Progress, Session, Task,
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
