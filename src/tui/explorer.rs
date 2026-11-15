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

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for root in &self.roots {
            self.push(root, &display_root(root), 0, false, &mut rows);
        }
        rows
    }

    fn push(&self, path: &Path, name: &str, depth: usize, git: bool, rows: &mut Vec<Row>) {
        if let Some(folder) = self.attached_at(path) {
            self.push_root(folder, name, depth, git, rows);
            return;
        }
        let open = self.expanded.contains(path);
        let contains = self
            .attached
            .iter()
            .any(|folder| project::is_inside(&folder.path, path));
        let mut notes = Vec::new();
        if git {
            notes.push("git".to_owned());
        }
        if let Some((_, owner)) = self.owner(path) {
            notes.push(format!("in project {owner}"));
        }
        rows.push(Row {
            depth,
            arrow: if open { Arrow::Open } else { Arrow::Closed },
            mark: if contains { Mark::Partial } else { Mark::Off },
            name: name.to_owned(),
            note: notes.join(", "),
            size: None,
            total: None,
            measuring: false,
            target: Target::Outside(path.to_path_buf()),
        });
        if open {
            for dir in self.listings.get(path).into_iter().flatten() {
                let name = dir
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.push(&dir.path, &name, depth + 1, dir.git, rows);
            }
        }
    }

    fn push_root(
        &self,
        folder: &Attached,
        name: &str,
        depth: usize,
        git: bool,
        rows: &mut Vec<Row>,
    ) {
        let open = self.expanded.contains(&folder.path);
        let tree = self.tree(&folder.path);
        let mark = match content::check("", &folder.skip) {
            Check::Partial => Mark::Partial,
            _ => Mark::On,
        };
        rows.push(Row {
            depth,
            arrow: if open { Arrow::Open } else { Arrow::Closed },
            mark,
            name: name.to_owned(),
            note: if git { "git".to_owned() } else { String::new() },
            size: tree.map(|tree| tree.uploaded(&folder.skip)),
            total: tree.map(|tree| tree.size),
            measuring: tree.is_none(),
            target: Target::Root(folder.path.clone()),
        });
        if let (true, Some(tree)) = (open, tree) {
            self.push_inside(folder, tree, depth + 1, rows);
        }
    }

    fn push_inside(&self, folder: &Attached, entry: &Entry, depth: usize, rows: &mut Vec<Row>) {
        for child in entry.children.iter().flatten() {
            let full = folder.path.join(&child.path);
            let is_dir = child.children.is_some();
            let open = is_dir && self.expanded.contains(&full);
            let mark = if child.ignored {
                Mark::Ignored
            } else {
                match content::check(&child.path, &folder.skip) {
                    Check::On => Mark::On,
                    Check::Partial => Mark::Partial,
                    Check::Off => Mark::Off,
                }
            };
            rows.push(Row {
                depth,
                arrow: match (is_dir && !child.ignored, open) {
                    (false, _) => Arrow::None,
                    (true, false) => Arrow::Closed,
                    (true, true) => Arrow::Open,
                },
                mark,
                name: child.name.clone(),
                note: match (child.ignored, child.suggested) {
                    (true, _) => "ignored by .saiignore".to_owned(),
                    (false, Some(reason)) => reason.to_owned(),
                    (false, None) => String::new(),
                },
                size: (!child.ignored).then_some(child.size),
                total: None,
                measuring: false,
                target: Target::Inside(folder.path.clone(), child.path.clone()),
            });
            if open && !child.ignored {
                self.push_inside(folder, child, depth + 1, rows);
            }
        }
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

    fn toggle_at(&mut self, index: usize) {
        if let Some((target, mark)) = self
            .rows()
            .get(index)
            .map(|row| (row.target.clone(), row.mark))
        {
            self.toggle(target, mark);
        }
    }

    fn open_or_close(&mut self, index: usize) {
        let Some((target, arrow)) = self
            .rows()
            .get(index)
            .map(|row| (row.target.clone(), row.arrow))
        else {
            return;
        };
        let path = full_path(&target);
        match arrow {
            Arrow::Closed => self.expand(&path),
            Arrow::Open => {
                self.expanded.remove(&path);
            }
            Arrow::None => {}
        }
    }

    fn toggle(&mut self, target: Target, mark: Mark) {
        match target {
            Target::Inside(root, path) => {
                if mark == Mark::Ignored {
                    self.message = Some(
                        "this is ignored by .saiignore, edit that file to upload it".to_owned(),
                    );
                    return;
                }
                let Some(Scan::Ready(tree, _)) = self.scans.get(&root) else {
                    return;
                };
                if let Some(folder) = self
                    .attached
                    .iter_mut()
                    .find(|folder| project::same_path(&folder.path, &root))
                {
                    content::toggle(tree, &mut folder.skip, &path);
                }
            }
            Target::Root(path) => {
                let index = self
                    .attached
                    .iter()
                    .position(|folder| project::same_path(&folder.path, &path));
                if let Some(index) = index {
                    if mark == Mark::Partial {
                        self.attached[index].skip.clear();
                    } else {
                        self.attached.remove(index);
                    }
                }
            }
            Target::Outside(path) => {
                if let Err(reason) = self.can_attach(&path) {
                    self.message = Some(reason);
                    return;
                }
                self.attached.push(Attached {
                    path: path.clone(),
                    skip: BTreeSet::new(),
                    suggest: true,
                });
                self.measure(&path);
                self.apply_suggestions();
            }
        }
    }

    fn can_attach(&self, path: &Path) -> Result<(), String> {
        if project::ensure_attachable(path).is_err() {
            return Err(format!(
                "{} is your home folder or a whole disk, open it with → and check a project folder inside",
                path.display()
            ));
        }
        if let Some((_, owner)) = self.owner(path) {
            return Err(format!("{} is already in project {owner}", path.display()));
        }
        if let Some((other, owner)) = self
            .others
            .iter()
            .find(|(other, _)| project::is_inside(other, path))
        {
            return Err(format!(
                "{} contains {}, which is in project {owner}",
                path.display(),
                other.display()
            ));
        }
        if let Some(inner) = self
            .attached
            .iter()
            .find(|folder| project::is_inside(&folder.path, path))
        {
            return Err(format!(
                "{} contains {}, which is already checked. Uncheck it first",
                path.display(),
                inner.path.display()
            ));
        }
        Ok(())
    }

    fn plan(&mut self) -> Step {
        let mut plan = Plan::default();
        for folder in &self.original {
            if !self
                .attached
                .iter()
                .any(|current| project::same_path(&current.path, &folder.path))
            {
                plan.remove.push(folder.path.clone());
            }
        }
        let mut names: Vec<String> = self
            .original
            .iter()
            .filter(|folder| !plan.remove.contains(&folder.path))
            .map(|folder| folder_name(&folder.path))
            .collect();
        for folder in &self.attached {
            let original = self
                .original
                .iter()
                .find(|item| project::same_path(&item.path, &folder.path));
            if original.is_some_and(|item| item.skip == folder.skip) {
                continue;
            }
            let Some(tree) = self.tree(&folder.path) else {
                self.message = Some(format!(
                    "still measuring {}, try again in a moment",
                    folder.path.display()
                ));
                return Step::Continue;
            };
            if tree.nothing_selected(&folder.skip) {
                self.message = Some(format!(
                    "nothing is checked inside {}, uncheck the whole folder instead",
                    folder.path.display()
                ));
                return Step::Continue;
            }
            let selection = tree.selection(&folder.skip);
            if original.is_some() {
                plan.change.push(Change {
                    path: folder.path.clone(),
                    selection,
                });
            } else {
                let alias = unique(&folder_name(&folder.path), &names);
                names.push(alias.clone());
                plan.add.push(Addition {
                    path: folder.path.clone(),
                    alias,
                    selection,
                });
            }
        }
        if plan.is_empty() && self.original.is_empty() {
            self.message = Some("check at least one folder with Space".to_owned());
            return Step::Continue;
        }
        Step::Apply(plan)
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

    pub fn new_repositories(&self, plan: &Plan) -> Vec<(PathBuf, Vec<PathBuf>)> {
        plan.add
            .iter()
            .filter_map(|item| match self.scans.get(&item.path) {
                Some(Scan::Ready(_, found)) if !found.is_empty() => {
                    Some((item.path.clone(), found.clone()))
                }
                _ => None,
            })
            .collect()
    }

    fn attached_at(&self, path: &Path) -> Option<&Attached> {
        self.attached
            .iter()
            .find(|folder| project::same_path(&folder.path, path))
    }

    fn owner(&self, path: &Path) -> Option<&(PathBuf, String)> {
        self.others
            .iter()
            .find(|(other, _)| project::same_path(other, path) || project::is_inside(path, other))
    }

    fn tree(&self, path: &Path) -> Option<&Entry> {
        match self.scans.get(path) {
            Some(Scan::Ready(tree, _)) => Some(tree),
            _ => None,
        }
    }

    fn measure(&mut self, path: &Path) {
        if self.scans.contains_key(path) {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let target = path.to_path_buf();
        thread::spawn(move || {
            sender
                .send((content::scan(&target), repos::find(&target)))
                .ok();
        });
        self.scans
            .insert(path.to_path_buf(), Scan::Pending(receiver));
    }

    fn expand(&mut self, path: &Path) {
        if self.attached_at(path).is_none()
            && !self
                .attached
                .iter()
                .any(|folder| project::is_inside(path, &folder.path))
        {
            self.listings
                .entry(path.to_path_buf())
                .or_insert_with(|| list_dirs(path));
        }
        self.expanded.insert(path.to_path_buf());
    }

    fn include_root(&mut self, path: &Path) {
        if !self
            .roots
            .iter()
            .any(|root| project::same_path(root, path) || project::is_inside(path, root))
        {
            self.roots.push(path.to_path_buf());
        }
    }

    fn reveal_parents(&mut self, path: &Path) {
        let Some(root) = self
            .roots
            .iter()
            .find(|root| project::is_inside(path, root))
            .cloned()
        else {
            return;
        };
        let mut chain: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .take_while(|ancestor| {
                project::is_inside(ancestor, &root) || project::same_path(ancestor, &root)
            })
            .map(Path::to_path_buf)
            .collect();
        chain.reverse();
        for ancestor in chain {
            self.expand(&ancestor);
        }
    }

    fn point_at(&mut self, path: &Path) {
        if let Some(index) = self
            .rows()
            .iter()
            .position(|row| project::same_path(&full_path(&row.target), path))
        {
            self.cursor = index;
        }
    }
}

fn full_path(target: &Target) -> PathBuf {
    match target {
        Target::Outside(path) | Target::Root(path) => path.clone(),
        Target::Inside(root, path) => root.join(path),
    }
}

fn folder_name(path: &Path) -> String {
    project::folder_name(path).unwrap_or_else(|_| "folder".to_owned())
}

fn unique(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|existing| existing == name) {
        return name.to_owned();
    }
    (2..)
        .map(|number| format!("{name}-{number}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_owned())
}

fn list_dirs(path: &Path) -> Vec<Dir> {
    let mut dirs: Vec<Dir> = fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    !name.starts_with('.')
                        && !content::hidden(&name, true)
                        && !hidden_by_system(entry)
                })
                .map(|entry| {
                    let path = entry.path();
                    Dir {
                        git: path.join(repos::GIT_DIR).is_dir(),
                        path,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    dirs.sort_by_key(|dir| {
        dir.path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    dirs
}

#[cfg(windows)]
fn hidden_by_system(entry: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    entry
        .metadata()
        .is_ok_and(|metadata| metadata.file_attributes() & (HIDDEN | SYSTEM) != 0)
}

#[cfg(not(windows))]
fn hidden_by_system(_: &fs::DirEntry) -> bool {
    false
}

fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = etcetera::home_dir().into_iter().collect();
    if cfg!(windows) {
        roots.extend(drives());
    } else {
        roots.push(PathBuf::from("/"));
    }
    roots
}

fn drives() -> Vec<PathBuf> {
    let (sender, receiver) = mpsc::channel();
    for letter in 'C'..='Z' {
        let sender = sender.clone();
        thread::spawn(move || {
            let drive = PathBuf::from(format!("{letter}:\\"));
            if drive.is_dir() {
                sender.send(drive).ok();
            }
        });
    }
    drop(sender);
    let deadline = Instant::now() + DRIVE_WAIT;
    let mut found = Vec::new();
    while let Ok(drive) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        found.push(drive);
    }
    found.sort();
    found
}

fn display_root(path: &Path) -> String {
    match etcetera::home_dir() {
        Ok(home) if project::same_path(&home, path) => "~".to_owned(),
        _ => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ratatui::crossterm::event::{
        KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;

    use super::{Explorer, Step, unique};

    #[test]
    fn picks_unique_folder_names() {
        let taken = vec!["src".to_owned(), "src-2".to_owned()];
        assert_eq!(unique("app", &taken), "app");
        assert_eq!(unique("src", &taken), "src-3");
    }

    #[test]
    fn checks_folders_and_their_content() {
        let root = std::env::temp_dir().join(format!("sai-explorer-{}", std::process::id()));
        fs::create_dir_all(root.join("front/src")).unwrap();
        fs::create_dir_all(root.join("front/data")).unwrap();
        fs::create_dir_all(root.join("back/.git")).unwrap();
        fs::write(root.join("front/src/app.ts"), [0u8; 10]).unwrap();
        fs::write(root.join("front/data/big.bin"), [0u8; 90]).unwrap();
        let root = fs::canonicalize(&root).unwrap();

        let mut explorer = Explorer::open("shop-test-explorer");
        explorer.roots = vec![root.clone()];
        explorer.expanded.clear();
        explorer.expand(&root);
        explorer.cursor = 0;
        let names: Vec<String> = explorer.rows().iter().map(|row| row.name.clone()).collect();
        assert_eq!(names[1..], ["back", "front"]);
        assert_eq!(explorer.rows()[1].note, "git");

        explorer.key(KeyCode::Down);
        explorer.key(KeyCode::Down);
        explorer.key(KeyCode::Char(' '));
        for _ in 0..200 {
            explorer.tick();
            if explorer.uploaded() == 100 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(explorer.uploaded(), 100);
        explorer.key(KeyCode::Right);
        let names: Vec<String> = explorer.rows().iter().map(|row| row.name.clone()).collect();
        assert_eq!(names[3..], ["data", "src"]);
        explorer.key(KeyCode::Down);
        explorer.key(KeyCode::Char(' '));
        assert_eq!(explorer.uploaded(), 10);

        explorer.cursor = 0;
        explorer.key(KeyCode::Char(' '));
        assert!(explorer.message.is_some());

        explorer.area.set(Rect::new(0, 0, 60, 10));
        explorer.offset.set(0);
        let click = |column, row| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        explorer.mouse(click(3 + 2, 2));
        assert_eq!(explorer.cursor, 1);
        assert_eq!(explorer.rows().len(), 5);
        explorer.mouse(click(3 + 6, 4));
        assert_eq!(explorer.uploaded(), 100);
        explorer.mouse(click(3 + 6, 4));
        assert_eq!(explorer.uploaded(), 10);

        let Step::Apply(plan) = explorer.key(KeyCode::Enter) else {
            panic!("expected a plan");
        };
        assert_eq!(plan.add.len(), 1);
        assert_eq!(plan.add[0].alias, "front");
        assert_eq!(plan.add[0].selection.skip, ["data"]);
        assert_eq!(plan.add[0].selection.uploaded, 10);
        fs::remove_dir_all(root).unwrap();
    }
}
