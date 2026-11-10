use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
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
struct Conflict {
    #[serde(default)]
    root: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    scan_problems: Vec<Problem>,
    #[serde(default)]
    transition_problems: Vec<Problem>,
}

#[derive(Deserialize)]
struct Problem {
    #[serde(default)]
    path: String,
    #[serde(default)]
    error: String,
}

impl Session {
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
        let disconnected = [("local folder", &self.alpha), ("VPS", &self.beta)]
            .into_iter()
            .filter(|(_, endpoint)| !self.paused && !endpoint.connected)
            .map(|(side, _)| format!("sync is not connected to the {side}"));
        let conflicts = self.conflicts.iter().map(|conflict| {
            format!(
                "sync conflict at {}, run `sai resolve <dir>` to keep the local version",
                conflict.root
            )
        });
        let problems = [("local", &self.alpha), ("VPS", &self.beta)]
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
