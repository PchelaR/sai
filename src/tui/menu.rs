#[derive(Clone, Copy)]
pub enum General {
    Attach,
    Update,
    Doctor,
    Logs,
    Reset,
    Refresh,
    Quit,
}

impl General {
    pub const ALL: [General; 7] = [
        General::Attach,
        General::Update,
        General::Doctor,
        General::Logs,
        General::Reset,
        General::Refresh,
        General::Quit,
    ];

    pub fn command(self) -> &'static str {
        match self {
            General::Attach => "attach",
            General::Update => "update",
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
            General::Update => "Rebuild images",
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
            General::Update => 'u',
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
    Shell,
    Attach,
    Detach,
    Change,
    Snapshots,
    Rename,
    Stop,
    Remove,
    Purge,
}

impl ProjectItem {
    pub const ALL: [ProjectItem; 10] = [
        ProjectItem::Chat,
        ProjectItem::Shell,
        ProjectItem::Attach,
        ProjectItem::Detach,
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
            ProjectItem::Shell => "shell",
            ProjectItem::Attach => "attach",
            ProjectItem::Detach => "detach",
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
            ProjectItem::Chat => "Open the agent",
            ProjectItem::Shell => "Bash in the container",
            ProjectItem::Attach => "Add a folder",
            ProjectItem::Detach => "Remove a folder",
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
            ProjectItem::Shell => 's',
            ProjectItem::Attach => 'f',
            ProjectItem::Detach => 't',
            ProjectItem::Change => 'c',
            ProjectItem::Snapshots => 'p',
            ProjectItem::Rename => 'n',
            ProjectItem::Stop => 'x',
            ProjectItem::Remove => 'd',
            ProjectItem::Purge => 'D',
        }
    }
}
