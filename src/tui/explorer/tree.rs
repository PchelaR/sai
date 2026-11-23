//! Finding a path among the attached folders, measuring and expanding it
use super::*;

impl Explorer {
    pub(super) fn attached_at(&self, path: &Path) -> Option<&Attached> {
        self.attached
            .iter()
            .find(|folder| project::same_path(&folder.path, path))
    }

    pub(super) fn owner(&self, path: &Path) -> Option<&(PathBuf, String)> {
        self.others
            .iter()
            .find(|(other, _)| project::same_path(other, path) || project::is_inside(path, other))
    }

    pub(super) fn tree(&self, path: &Path) -> Option<&Entry> {
        match self.scans.get(path) {
            Some(Scan::Ready(tree, _)) => Some(tree),
            _ => None,
        }
    }

    pub(super) fn measure(&mut self, path: &Path) {
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

    pub(super) fn expand(&mut self, path: &Path) {
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

    pub(super) fn include_root(&mut self, path: &Path) {
        if !self
            .roots
            .iter()
            .any(|root| project::same_path(root, path) || project::is_inside(path, root))
        {
            self.roots.push(path.to_path_buf());
        }
    }

    pub(super) fn reveal_parents(&mut self, path: &Path) {
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

    pub(super) fn point_at(&mut self, path: &Path) {
        if let Some(index) = self
            .rows()
            .iter()
            .position(|row| project::same_path(&full_path(&row.target), path))
        {
            self.cursor = index;
        }
    }
}
