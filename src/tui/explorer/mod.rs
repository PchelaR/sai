use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::project::content::{self, Check, Entry};
use crate::project::folders::{Addition, Change, Plan};
use crate::project::{self, Registry, repos};

mod disk;
mod marks;
mod rows;
#[cfg(test)]
mod tests;
mod tree;

use disk::{display_root, folder_name, full_path, list_dirs, roots, unique};

const PAGE: usize = 10;
const DRIVE_WAIT: Duration = Duration::from_millis(300);
const WHEEL: usize = 3;
const HIGHLIGHT_WIDTH: u16 = 2;

enum Scan {
    Pending(Receiver<(Entry, Vec<PathBuf>)>),
    Ready(Entry, Vec<PathBuf>),
}

struct Attached {
    path: PathBuf,
    skip: BTreeSet<String>,
    suggest: bool,
}

pub struct Explorer {
    pub project: String,
    roots: Vec<PathBuf>,
    original: Vec<Attached>,
    attached: Vec<Attached>,
    others: Vec<(PathBuf, String)>,
    scans: BTreeMap<PathBuf, Scan>,
    listings: BTreeMap<PathBuf, Vec<Dir>>,
    expanded: BTreeSet<PathBuf>,
    pub cursor: usize,
    pub offset: Cell<usize>,
    pub area: Cell<Rect>,
    pub message: Option<String>,
    pub typing: Option<String>,
    pub completions: Vec<String>,
}

#[derive(Clone)]
struct Dir {
    path: PathBuf,
    git: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    On,
    Partial,
    Off,
    Ignored,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    None,
    Closed,
    Open,
}

pub struct Row {
    pub depth: usize,
    pub arrow: Arrow,
    pub mark: Mark,
    pub name: String,
    pub note: String,
    pub size: Option<u64>,
    pub total: Option<u64>,
    pub measuring: bool,
    target: Target,
}

#[derive(Clone)]
enum Target {
    Outside(PathBuf),
    Root(PathBuf),
    Inside(PathBuf, String),
}

pub enum Step {
    Continue,
    Apply(Plan),
    Cancel,
}

impl Explorer {
    pub fn open(project: &str) -> Self {
        let registry = Registry::load().ok();
        let mut original = Vec::new();
        let mut others = Vec::new();
        for found in registry.iter().flat_map(|registry| registry.projects()) {
            for folder in &found.folders {
                if found.name == project {
                    original.push(Attached {
                        path: folder.path.clone(),
                        skip: folder.skip.iter().cloned().collect(),
                        suggest: false,
                    });
                } else {
                    others.push((folder.path.clone(), found.name.clone()));
                }
            }
        }
        let attached = original
            .iter()
            .map(|folder| Attached {
                path: folder.path.clone(),
                skip: folder.skip.clone(),
                suggest: false,
            })
            .collect();
        let mut explorer = Self {
            project: project.to_owned(),
            roots: roots(),
            original,
            attached,
            others,
            scans: BTreeMap::new(),
            listings: BTreeMap::new(),
            expanded: BTreeSet::new(),
            cursor: 0,
            offset: Cell::new(0),
            area: Cell::new(Rect::default()),
            message: None,
            typing: None,
            completions: Vec::new(),
        };
        let paths: Vec<PathBuf> = explorer
            .attached
            .iter()
            .map(|folder| folder.path.clone())
            .collect();
        for path in &paths {
            explorer.measure(path);
            explorer.include_root(path);
            explorer.reveal_parents(path);
        }
        if let Ok(current) = project::current_dir() {
            explorer.include_root(&current);
            explorer.reveal_parents(&current);
            explorer.point_at(&current);
        }
        explorer
    }

    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        for scan in self.scans.values_mut() {
            if let Scan::Pending(receiver) = scan
                && let Ok((entry, repositories)) = receiver.try_recv()
            {
                *scan = Scan::Ready(entry, repositories);
                changed = true;
            }
        }
        if changed {
            self.apply_suggestions();
        }
        changed
    }

    fn apply_suggestions(&mut self) {
        for folder in &mut self.attached {
            if folder.suggest
                && let Some(Scan::Ready(tree, _)) = self.scans.get(&folder.path)
            {
                folder.skip = tree.suggested_skip();
                folder.suggest = false;
            }
        }
    }

    pub fn uploaded(&self) -> u64 {
        self.attached
            .iter()
            .filter_map(|folder| {
                self.tree(&folder.path)
                    .map(|tree| tree.uploaded(&folder.skip))
            })
            .sum()
    }

    pub fn key(&mut self, code: KeyCode) -> Step {
        if self.typing.is_some() {
            self.type_path(code);
            return Step::Continue;
        }
        self.message = None;
        let rows = self.rows();
        let last = rows.len().saturating_sub(1);
        let current = rows
            .get(self.cursor)
            .map(|row| (row.target.clone(), row.arrow, row.mark));
        drop(rows);
        match code {
            KeyCode::Esc => return Step::Cancel,
            KeyCode::Enter => return self.plan(),
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.cursor = (self.cursor + 1).min(last),
            KeyCode::PageUp => self.cursor = self.cursor.saturating_sub(PAGE),
            KeyCode::PageDown => self.cursor = (self.cursor + PAGE).min(last),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = last,
            KeyCode::Char('/') => self.typing = Some(String::new()),
            KeyCode::Char(' ') => self.toggle_at(self.cursor),
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some((target, Arrow::Closed, _)) = current {
                    self.expand(&full_path(&target));
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some((target, arrow, _)) = current {
                    let path = full_path(&target);
                    if arrow == Arrow::Open {
                        self.expanded.remove(&path);
                    } else if let Some(parent) = path.parent().map(Path::to_path_buf)
                        && self.expanded.remove(&parent)
                    {
                        self.point_at(&parent);
                    }
                }
            }
            _ => {}
        }
        self.cursor = self.cursor.min(self.rows().len().saturating_sub(1));
        Step::Continue
    }

    pub fn mouse(&mut self, event: MouseEvent) {
        if self.typing.is_some() {
            return;
        }
        let last = self.rows().len().saturating_sub(1);
        match event.kind {
            MouseEventKind::ScrollDown => self.cursor = (self.cursor + WHEEL).min(last),
            MouseEventKind::ScrollUp => self.cursor = self.cursor.saturating_sub(WHEEL),
            MouseEventKind::Down(MouseButton::Left) => {
                let area = self.area.get();
                let inside = event.column > area.x
                    && event.column < area.right().saturating_sub(1)
                    && event.row > area.y
                    && event.row < area.bottom().saturating_sub(1);
                if !inside {
                    return;
                }
                let index = self.offset.get() + usize::from(event.row - area.y - 1);
                let Some(depth) = self.rows().get(index).map(|row| row.depth) else {
                    return;
                };
                self.message = None;
                self.cursor = index;
                let column = usize::from(event.column.saturating_sub(area.x + 1 + HIGHLIGHT_WIDTH));
                let arrow = depth * 2;
                if column == arrow {
                    self.open_or_close(index);
                } else if (arrow + 2..arrow + 5).contains(&column) {
                    self.toggle_at(index);
                }
            }
            _ => {}
        }
    }

    fn type_path(&mut self, code: KeyCode) {
        let Some(mut value) = self.typing.take() else {
            return;
        };
        self.completions.clear();
        match code {
            KeyCode::Esc => return,
            KeyCode::Enter => {
                let path = super::app::expand_home(value.trim());
                match project::local_path(&path) {
                    Ok(path) => {
                        self.include_root(&path);
                        self.reveal_parents(&path);
                        self.point_at(&path);
                    }
                    Err(error) => {
                        self.message = Some(format!("{error:#}"));
                        self.typing = Some(value);
                    }
                }
                return;
            }
            KeyCode::Tab => {
                let completion =
                    super::complete::folder(&value, etcetera::home_dir().ok().as_deref());
                value = completion.value;
                self.completions = completion.options;
            }
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(character) => value.push(character),
            _ => {}
        }
        self.typing = Some(value);
    }
}
