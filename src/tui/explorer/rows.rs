//! The rows the explorer shows: folders around the computer and inside attached ones
use super::*;

impl Explorer {
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for root in &self.roots {
            self.push(root, &display_root(root), 0, false, &mut rows);
        }
        rows
    }

    pub(super) fn push(
        &self,
        path: &Path,
        name: &str,
        depth: usize,
        git: bool,
        rows: &mut Vec<Row>,
    ) {
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

    pub(super) fn push_root(
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

    pub(super) fn push_inside(
        &self,
        folder: &Attached,
        entry: &Entry,
        depth: usize,
        rows: &mut Vec<Row>,
    ) {
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
}
