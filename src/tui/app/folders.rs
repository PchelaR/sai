//! Choosing the folders of a project and what of them to upload
use super::*;

impl App {
    pub(super) fn explorer(&mut self, project: String, agent: Option<Agent>) -> Option<Outcome> {
        self.mode = Mode::Explorer {
            explorer: Box::new(Explorer::open(&project)),
            agent,
        };
        None
    }

    pub(super) fn explore(
        &mut self,
        mut explorer: Box<Explorer>,
        agent: Option<Agent>,
        key: KeyEvent,
    ) -> Option<Outcome> {
        match explorer.key(key.code) {
            Step::Cancel => None,
            Step::Continue => {
                self.mode = Mode::Explorer { explorer, agent };
                None
            }
            Step::Apply(plan) if plan.is_empty() => None,
            Step::Apply(plan) => {
                let repositories = explorer.new_repositories(&plan);
                let asks_git = !repositories.is_empty();
                let mut question = plan.summary();
                if asks_git {
                    question.push(' ');
                    question.push_str(&crate::attach::git_question_many(&repositories));
                } else {
                    question.push_str(" Apply?");
                }
                self.mode = Mode::Apply {
                    explorer,
                    agent,
                    plan,
                    question,
                    asks_git,
                };
                None
            }
        }
    }

    pub(super) fn apply(
        &mut self,
        explorer: Box<Explorer>,
        agent: Option<Agent>,
        plan: Plan,
        question: String,
        asks_git: bool,
        key: KeyEvent,
    ) -> Option<Outcome> {
        let git = match (key.code, asks_git) {
            (KeyCode::Char('y'), _) => asks_git,
            (KeyCode::Char('n'), true) => false,
            (KeyCode::Char('n') | KeyCode::Esc, _) => {
                self.mode = Mode::Explorer { explorer, agent };
                return None;
            }
            _ => {
                self.mode = Mode::Apply {
                    explorer,
                    agent,
                    plan,
                    question,
                    asks_git,
                };
                return None;
            }
        };
        run(Task::Folders {
            project: explorer.project.clone(),
            agent,
            plan,
            git,
        })
    }
}
