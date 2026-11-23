//! Snapshots to restore, and the review of files that can run code
use super::*;

impl App {
    pub(super) fn snapshots(
        &mut self,
        project: String,
        snapshots: Vec<Snapshot>,
        selected: usize,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let last = snapshots.len().saturating_sub(1);
        let selected = match key.code {
            KeyCode::Enter => {
                let snapshot = snapshots.get(selected)?.stamp.clone();
                return self.ask(Input::RestorePath { project, snapshot }, String::new());
            }
            KeyCode::Esc => return None,
            KeyCode::Down | KeyCode::Char('j') => (selected + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            _ => selected,
        };
        self.mode = Mode::Snapshots {
            project,
            snapshots,
            selected,
        };
        None
    }

    pub(super) fn load_review(&mut self, project: String) {
        let (sender, receiver) = mpsc::channel();
        let name = project.clone();
        thread::spawn(move || {
            let result = review::unreviewed(&name)
                .map(|changes| review::lines(&changes))
                .map_err(|error| format!("{error:#}"));
            sender.send(result).ok();
        });
        self.mode = Mode::LoadingReview { project, receiver };
    }

    pub(super) fn mark_reviewed(&mut self, project: &str) {
        let name = project.to_owned();
        thread::spawn(move || {
            review::mark(&name).ok();
        });
        if let Some(loaded) = &mut self.loaded
            && let Some(row) = loaded.projects.iter_mut().find(|row| row.name == project)
        {
            row.review = 0;
        }
    }
}
