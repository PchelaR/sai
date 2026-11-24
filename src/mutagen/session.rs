use serde::Deserialize;

use crate::project::size::human;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub ignore: Ignore,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    conflicts: Vec<Conflict>,
    #[serde(default)]
    alpha: Endpoint,
    #[serde(default)]
    beta: Endpoint,
}

#[derive(Default, Deserialize)]
pub struct Ignore {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conflict {
    #[serde(default)]
    root: String,
    #[serde(default)]
    alpha_changes: Vec<Change>,
}

#[derive(Deserialize)]
struct Change {
    #[serde(default)]
    path: String,
    #[serde(default)]
    new: Option<serde_json::Value>,
}

impl Conflict {
    /// The VPS side deleted the whole folder, and mutagen stopped because the local one
    /// still has something it does not sync: build output, or files of the user
    fn deleted_on_alpha(&self) -> bool {
        self.alpha_changes
            .iter()
            .any(|change| change.path == self.root && change.new.is_none())
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    #[serde(default)]
    path: String,
    #[serde(default)]
    staging_progress: Option<Staging>,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    scan_problems: Vec<Problem>,
    #[serde(default)]
    transition_problems: Vec<Problem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Staging {
    #[serde(default)]
    received_files: u64,
    #[serde(default)]
    expected_files: u64,
    #[serde(default)]
    total_received_size: u64,
}

#[derive(Deserialize)]
struct Problem {
    #[serde(default)]
    path: String,
    #[serde(default)]
    error: String,
}

impl Session {
    pub fn local_path(&self) -> &str {
        &self.local().path
    }

    /// Folders the agent deleted that are still in the local copy, relative to its root
    pub fn deleted_by_agent(&self) -> Vec<&str> {
        if !self.agent_wins() {
            return Vec::new();
        }
        self.conflicts
            .iter()
            .filter(|conflict| conflict.deleted_on_alpha())
            .map(|conflict| conflict.root.as_str())
            .collect()
    }

    fn agent_wins(&self) -> bool {
        self.mode == super::TWO_WAY
    }

    fn local(&self) -> &Endpoint {
        if self.agent_wins() {
            &self.beta
        } else {
            &self.alpha
        }
    }

    fn vps(&self) -> &Endpoint {
        if self.agent_wins() {
            &self.alpha
        } else {
            &self.beta
        }
    }

    pub fn progress(&self) -> String {
        let staging = self
            .beta
            .staging_progress
            .as_ref()
            .or(self.alpha.staging_progress.as_ref());
        match (self.status.as_str(), staging) {
            ("staging-alpha" | "staging-beta", Some(staging)) => format!(
                "uploading: {} of {} files, {} so far",
                staging.received_files,
                staging.expected_files,
                human(staging.total_received_size)
            ),
            ("staging-alpha" | "staging-beta", None) => "uploading files".to_owned(),
            ("connecting-alpha" | "connecting-beta" | "disconnected", _) => {
                "connecting to the VPS".to_owned()
            }
            ("scanning" | "waiting-for-rescan", _) => "looking through the files".to_owned(),
            ("reconciling", _) => "comparing files".to_owned(),
            ("transitioning", _) => "writing files on the VPS".to_owned(),
            ("saving" | "watching", _) => "finishing".to_owned(),
            (other, _) => other.replace('-', " "),
        }
    }

    pub fn summary(&self) -> String {
        let state: &str = if self.paused { "paused" } else { &self.status };
        match self.issues().len() {
            0 => state.to_owned(),
            count => format!("{state}, {count} issue(s)"),
        }
    }

    pub fn issues(&self) -> Vec<String> {
        let errors = self
            .last_error
            .iter()
            .map(|error| format!("sync error: {error}"));
        let disconnected = [("local folder", self.local()), ("VPS", self.vps())]
            .into_iter()
            .filter(|(_, endpoint)| !self.paused && !endpoint.connected)
            .map(|(side, _)| format!("sync is not connected to the {side}"));
        let conflicts = self.conflicts.iter().map(|conflict| {
            if self.agent_wins() && conflict.deleted_on_alpha() {
                // `sai resolve` would bring the folder back to the VPS, not what the agent did
                format!(
                    "the agent deleted {}, but your copy keeps files there that sai does not sync, delete the folder yourself if you do not need them",
                    conflict.root
                )
            } else {
                format!(
                    "sync conflict at {}, run `sai resolve <project>` to keep the local versions",
                    conflict.root
                )
            }
        });
        let problems = [("local", self.local()), ("VPS", self.vps())]
            .into_iter()
            .flat_map(|(side, endpoint)| {
                endpoint
                    .scan_problems
                    .iter()
                    .chain(&endpoint.transition_problems)
                    .map(move |problem| {
                        format!(
                            "sync problem on {side} at {}: {}",
                            problem.path, problem.error
                        )
                    })
            });
        errors
            .chain(disconnected)
            .chain(conflicts)
            .chain(problems)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Session;

    const WATCHING: &str = r#"{"name":"sai-app","paused":false,"status":"watching","ignore":{"paths":["target/"],"vcs":true},"alpha":{"connected":true},"beta":{"connected":true},"successfulCycles":4}"#;

    #[test]
    fn knows_which_side_is_the_vps() {
        let agent_wins: Session = serde_json::from_str(
            r#"{"name":"sai-app","mode":"two-way-resolved","status":"connecting-alpha","alpha":{"path":"/home/sai/projects/app/front","connected":false},"beta":{"path":"/code/front","connected":true}}"#,
        )
        .unwrap();
        assert_eq!(agent_wins.local_path(), "/code/front");
        assert_eq!(agent_wins.issues(), ["sync is not connected to the VPS"]);
        let local_wins: Session = serde_json::from_str(
            r#"{"name":"sai-app","mode":"two-way-safe","alpha":{"path":"/code/front","connected":true},"beta":{"connected":false}}"#,
        )
        .unwrap();
        assert_eq!(local_wins.local_path(), "/code/front");
        assert_eq!(local_wins.issues(), ["sync is not connected to the VPS"]);
    }

    #[test]
    fn parses_empty_session_lists() {
        assert!(super::super::parse_sessions("null").unwrap().is_empty());
        assert_eq!(
            super::super::parse_sessions(&format!("[{WATCHING}]"))
                .unwrap()
                .len(),
            1
        );
    }

    fn parse(json: &str) -> Session {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn healthy_session_has_no_issues() {
        let session = parse(WATCHING);
        assert_eq!(session.name, "sai-app");
        assert!(session.issues().is_empty());
        assert_eq!(session.summary(), "watching");
        assert_eq!(session.ignore.paths, ["target/"]);
    }

    #[test]
    fn describes_upload_progress() {
        let session = parse(
            r#"{"status":"staging-beta","beta":{"connected":true,"stagingProgress":{"receivedFiles":120,"expectedFiles":3400,"totalReceivedSize":1572864}}}"#,
        );
        assert_eq!(
            session.progress(),
            "uploading: 120 of 3400 files, 1.5 MB so far"
        );
        assert_eq!(
            parse(r#"{"status":"scanning"}"#).progress(),
            "looking through the files"
        );
        assert_eq!(
            parse(r#"{"status":"staging-beta"}"#).progress(),
            "uploading files"
        );
    }

    #[test]
    fn paused_session_is_not_reported_as_disconnected() {
        let session = parse(r#"{"paused":true,"status":"disconnected"}"#);
        assert!(session.issues().is_empty());
        assert_eq!(session.summary(), "paused");
    }

    #[test]
    fn knows_folders_the_agent_deleted() {
        let deleted = r#"{"root":"tests/functional","alphaChanges":[{"path":"tests/functional","old":{"kind":"directory"},"new":null}],"betaChanges":[{"path":"tests/functional/__pycache__","new":{"kind":"untracked"}}]}"#;
        let edited = r#"{"root":"a.txt","alphaChanges":[{"path":"a.txt","new":{"kind":"file"}}]}"#;
        let session = parse(&format!(
            r#"{{"mode":"two-way-resolved","status":"watching","conflicts":[{deleted},{edited}],"alpha":{{"connected":true}},"beta":{{"connected":true}}}}"#
        ));
        assert_eq!(session.deleted_by_agent(), ["tests/functional"]);
        let issues = session.issues();
        assert!(
            issues[0].contains("the agent deleted tests/functional"),
            "{issues:?}"
        );
        assert!(
            !issues[0].contains("sai resolve"),
            "resolve would bring it back"
        );
        assert!(issues[1].contains("sai resolve"));
        // Where the local side wins, the VPS deleting is not the agent's word
        let local_wins = parse(&format!(
            r#"{{"mode":"two-way-safe","conflicts":[{deleted}],"alpha":{{"connected":true}},"beta":{{"connected":true}}}}"#
        ));
        assert!(local_wins.deleted_by_agent().is_empty());
    }

    #[test]
    fn reports_every_problem() {
        let session = parse(
            r#"{"status":"watching","lastError":"boom","conflicts":[{"root":"a.txt"}],"alpha":{"connected":true,"scanProblems":[{"path":"b","error":"denied"}]},"beta":{"connected":false}}"#,
        );
        let issues = session.issues();
        assert_eq!(issues.len(), 4);
        assert!(issues.iter().any(|issue| issue.contains("boom")));
        assert!(issues.iter().any(|issue| issue.contains("a.txt")));
        assert!(issues.iter().any(|issue| issue.contains("denied")));
        assert!(issues.iter().any(|issue| issue.contains("VPS")));
        assert_eq!(session.summary(), "watching, 4 issue(s)");
    }
}
