#[derive(Clone, Copy)]
pub enum General {
    Attach,
    Vps,
    Update,
    Upgrade,
    Doctor,
    Logs,
    Reset,
    Refresh,
    Quit,
}

impl General {
    pub const ALL: [General; 9] = [
        General::Attach,
        General::Vps,
        General::Update,
        General::Upgrade,
        General::Doctor,
        General::Logs,
        General::Reset,
        General::Refresh,
        General::Quit,
    ];

    pub fn command(self) -> &'static str {
        match self {
            General::Attach => "attach",
            General::Vps => "vps",
            General::Update => "update",
            General::Upgrade => "upgrade",
            General::Doctor => "doctor",
            General::Logs => "logs",
            General::Reset => "reset",
            General::Refresh => "refresh",
            General::Quit => "quit",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            General::Attach => "Add a folder to a project",
            General::Vps => "Shell on the VPS",
            General::Update => "Rebuild images",
            General::Upgrade => "Install the latest sai",
            General::Doctor => "Check everything",
            General::Logs => "Save a report with logs",
            General::Reset => "Delete everything on the VPS",
            General::Refresh => "Reload the list",
            General::Quit => "Exit sai",
        }
    }

    pub fn key(self) -> char {
        match self {
            General::Attach => 'a',
            General::Vps => 'v',
            General::Update => 'u',
            General::Upgrade => 'U',
            General::Doctor => 'g',
            General::Logs => 'l',
            General::Reset => 'R',
            General::Refresh => 'r',
            General::Quit => 'q',
        }
    }
}

#[derive(Clone, Copy)]
pub enum ProjectItem {
    Chat,
    RemoveChat,
    Shell,
    Folders,
    Services,
    History,
    Change,
    Snapshots,
    Rename,
    Stop,
    Remove,
    Purge,
}

impl ProjectItem {
    pub const ALL: [ProjectItem; 12] = [
        ProjectItem::Chat,
        ProjectItem::RemoveChat,
        ProjectItem::Shell,
        ProjectItem::Folders,
        ProjectItem::Services,
        ProjectItem::History,
        ProjectItem::Change,
        ProjectItem::Snapshots,
        ProjectItem::Rename,
        ProjectItem::Stop,
        ProjectItem::Remove,
        ProjectItem::Purge,
    ];

    pub fn command(self) -> &'static str {
        match self {
            ProjectItem::Chat => "chat",
            ProjectItem::RemoveChat => "rm chat",
            ProjectItem::Shell => "shell",
            ProjectItem::Folders => "folders",
            ProjectItem::Services => "services",
            ProjectItem::History => "history",
            ProjectItem::Change => "change",
            ProjectItem::Snapshots => "snapshots",
            ProjectItem::Rename => "rename",
            ProjectItem::Stop => "stop",
            ProjectItem::Remove => "rm",
            ProjectItem::Purge => "rm --purge",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ProjectItem::Chat => "Continue a chat or start a new one",
            ProjectItem::RemoveChat => "Delete a chat",
            ProjectItem::Shell => "Bash in the container",
            ProjectItem::Folders => "Choose folders and what to upload",
            ProjectItem::Services => "Databases and queues next to the agent",
            ProjectItem::History => "Choose repositories with git history",
            ProjectItem::Change => "Switch to another agent",
            ProjectItem::Snapshots => "List and restore",
            ProjectItem::Rename => "Give the project another name",
            ProjectItem::Stop => "Pause sync and container",
            ProjectItem::Remove => "Remove the project, keep VPS files",
            ProjectItem::Purge => "Remove with files on the VPS",
        }
    }

    pub fn key(self) -> char {
        match self {
            ProjectItem::Chat => 'o',
            ProjectItem::RemoveChat => 'K',
            ProjectItem::Shell => 's',
            ProjectItem::Folders => 'f',
            ProjectItem::Services => 'e',
            ProjectItem::History => 'h',
            ProjectItem::Change => 'c',
            ProjectItem::Snapshots => 'p',
            ProjectItem::Rename => 'n',
            ProjectItem::Stop => 'x',
            ProjectItem::Remove => 'd',
            ProjectItem::Purge => 'D',
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{General, ProjectItem};

    /// The list takes both kinds of keys at once: a shared key would hide one of them
    #[test]
    fn every_key_does_one_thing() {
        let keys: Vec<char> = General::ALL
            .iter()
            .map(|item| item.key())
            .chain(ProjectItem::ALL.iter().map(|item| item.key()))
            .collect();
        let unique: HashSet<char> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len(), "{keys:?}");
        for key in ['j', 'k', '?'] {
            assert!(
                !unique.contains(&key),
                "{key} moves the cursor or opens the help"
            );
        }
        let commands: HashSet<&str> = General::ALL
            .iter()
            .map(|item| item.command())
            .chain(ProjectItem::ALL.iter().map(|item| item.command()))
            .collect();
        assert_eq!(commands.len(), keys.len());
        for item in ProjectItem::ALL {
            assert!(!item.description().is_empty());
        }
        for item in General::ALL {
            assert!(!item.description().is_empty());
        }
    }
}
