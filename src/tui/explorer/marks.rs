//! What the user marks to attach, upload or skip, and the plan made of it
use super::*;

impl Explorer {
    pub(super) fn toggle_at(&mut self, index: usize) {
        if let Some((target, mark)) = self
            .rows()
            .get(index)
            .map(|row| (row.target.clone(), row.mark))
        {
            self.toggle(target, mark);
        }
    }

    pub(super) fn open_or_close(&mut self, index: usize) {
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

    pub(super) fn toggle(&mut self, target: Target, mark: Mark) {
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

    pub(super) fn can_attach(&self, path: &Path) -> Result<(), String> {
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

    pub(super) fn plan(&mut self) -> Step {
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
}
