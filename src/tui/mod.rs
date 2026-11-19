mod actions;
mod app;
mod bell;
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
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
    KeyEventKind, MouseEventKind,
};
use ratatui::crossterm::{cursor, execute};

use crate::project::Registry;
use crate::{console, mutagen, output};
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
    let mut watcher = Some(bell::watch());
    loop {
        if let Some(watching) = &watcher {
            for states in watching.states.try_iter() {
                if app.agent_states(states) && app.unfocused {
                    bell::ring();
                }
                dirty = true;
            }
        }
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
        let outcome = match app.pending.take() {
            Some(outcome) => Some(outcome),
            None => {
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
                    Event::Resize(..) => {
                        dirty = true;
                        continue;
                    }
                    Event::FocusGained => {
                        app.unfocused = false;
                        dirty = true;
                        continue;
                    }
                    Event::FocusLost => {
                        app.unfocused = true;
                        continue;
                    }
                    _ => continue,
                };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                app.handle(key)
            }
        };
        dirty = true;
        match outcome {
            Some(Outcome::Quit) => return Ok(()),
            Some(Outcome::Refresh) => {
                progress = None;
                retry_at = None;
                loading = reload(&mut app);
            }
            Some(Outcome::Run(Action::Open(session))) => {
                drop(watcher.take());
                actions::open(terminal, session)?;
                terminal.clear()?;
                app.forget_agent_states();
                watcher = Some(bell::watch());
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
    output::take_screen(true);
    execute!(io::stdout(), EnableMouseCapture, EnableFocusChange)?;
    Ok(terminal)
}

pub fn restore() {
    output::take_screen(false);
    execute!(io::stdout(), DisableMouseCapture, DisableFocusChange).ok();
    ratatui::restore();
    // ratatui hides the cursor while drawing and does not show it back, so a plain shell
    // opened from the menu (sai vps, sai shell) would have no cursor
    execute!(io::stdout(), cursor::Show).ok();
}

fn reload(app: &mut App) -> Receiver<Result<Loaded, String>> {
    app.loading = true;
    loader::spawn()
}
