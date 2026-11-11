mod actions;
mod app;
mod loader;
mod menu;
mod view;

use std::{sync::mpsc::Receiver, time::Duration};

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::{console, mutagen};
use actions::{Action, Progress};
use app::{App, Outcome};
use loader::Loaded;

const TICK: Duration = Duration::from_millis(100);

pub fn run() -> Result<()> {
    console::ensure_supported()?;
    mutagen::ensure_installed()?;
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::default();
    let mut loading = reload(&mut app);
    let mut progress: Option<Receiver<Progress>> = None;
    loop {
        if let Ok(loaded) = loading.try_recv() {
            app.update(loaded);
        }
        if let Some(receiver) = &progress {
            for update in receiver.try_iter() {
                app.progress(update);
            }
        }
        terminal.draw(|frame| view::render(frame, &app))?;
        if !event::poll(TICK)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle(key) {
            Some(Outcome::Quit) => return Ok(()),
            Some(Outcome::Refresh) => {
                progress = None;
                loading = reload(&mut app);
            }
            Some(Outcome::Run(Action::Open(session))) => {
                actions::open(terminal, session)?;
                loading = reload(&mut app);
            }
            Some(Outcome::Run(Action::Run(task))) => {
                app.start(task.title());
                progress = Some(actions::spawn(task));
            }
            None => {}
        }
    }
}

fn reload(app: &mut App) -> Receiver<Result<Loaded, String>> {
    app.loading = true;
    loader::spawn()
}
