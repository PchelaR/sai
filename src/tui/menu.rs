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
    NewChat,
    RemoveChat,
    Shell,
    Folders,
    Change,
    Snapshots,
    Rename,
    Stop,
    Remove,
    Purge,
}

impl ProjectItem {
    pub const ALL: [ProjectItem; 11] = [
        ProjectItem::Chat,
        ProjectItem::NewChat,
        ProjectItem::RemoveChat,
        ProjectItem::Shell,
        ProjectItem::Folders,
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
            ProjectItem::NewChat => "new chat",
            ProjectItem::RemoveChat => "rm chat",
            ProjectItem::Shell => "shell",
            ProjectItem::Folders => "folders",
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
            ProjectItem::NewChat => "Start another chat",
            ProjectItem::RemoveChat => "Delete a chat",
            ProjectItem::Shell => "Bash in the container",
            ProjectItem::Folders => "Choose folders and what to upload",
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
            ProjectItem::NewChat => 'N',
            ProjectItem::RemoveChat => 'K',
            ProjectItem::Shell => 's',
            ProjectItem::Folders => 'f',
            ProjectItem::Change => 'c',
            ProjectItem::Snapshots => 'p',
            ProjectItem::Rename => 'n',
            ProjectItem::Stop => 'x',
            ProjectItem::Remove => 'd',
            ProjectItem::Purge => 'D',
        }
    }
}
