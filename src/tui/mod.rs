mod actions;
mod app;
mod complete;
mod explorer;
mod loader;
mod menu;
mod view;

use std::io;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use ratatui::crossterm::execute;

use crate::project::Registry;
use crate::{console, mutagen};
use actions::{Action, Progress};
use app::{App, Mode, Outcome};
use loader::Loaded;

const TICK: Duration = Duration::from_millis(100);
const RELOAD_AFTER_FAILURE: Duration = Duration::from_secs(15);

pub fn run() -> Result<()> {
    console::ensure_supported()?;
    mutagen::ensure_installed()?;
    Registry::open()?;
    let mut terminal = init()?;
    let result = event_loop(&mut terminal);
    restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::default();
    let mut loading = reload(&mut app);
    let mut progress: Option<Receiver<Progress>> = None;
    let mut dirty = true;
    let mut retry_at: Option<Instant> = None;
    loop {
        if let Ok(loaded) = loading.try_recv() {
            retry_at = loaded
                .is_err()
                .then(|| Instant::now() + RELOAD_AFTER_FAILURE);
            app.update(loaded);
            dirty = true;
        }
        if retry_at.is_some_and(|at| Instant::now() >= at) && matches!(app.mode, Mode::Browse) {
            retry_at = None;
            loading = reload(&mut app);
            dirty = true;
        }
        if let Some(receiver) = &progress {
            for update in receiver.try_iter() {
                app.progress(update);
                dirty = true;
            }
        }
        dirty |= app.tick();
        if dirty {
            terminal.draw(|frame| view::render(frame, &app))?;
            dirty = false;
        }
        if !event::poll(TICK)? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) => key,
            Event::Mouse(mouse) => {
                if mouse.kind != MouseEventKind::Moved {
                    app.mouse(mouse);
                    dirty = true;
                }
                continue;
            }
            Event::Resize(..) | Event::FocusGained => {
                dirty = true;
                continue;
            }
            _ => continue,
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        dirty = true;
        match app.handle(key) {
            Some(Outcome::Quit) => return Ok(()),
            Some(Outcome::Refresh) => {
                progress = None;
                retry_at = None;
                loading = reload(&mut app);
            }
            Some(Outcome::Run(Action::Open(session))) => {
                actions::open(terminal, session)?;
                terminal.clear()?;
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

pub fn init() -> Result<DefaultTerminal> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    Ok(terminal)
}

pub fn restore() {
    execute!(io::stdout(), DisableMouseCapture).ok();
    ratatui::restore();
}

fn reload(app: &mut App) -> Receiver<Result<Loaded, String>> {
    app.loading = true;
    loader::spawn()
}
