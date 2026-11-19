mod agent;
mod attach;
mod chat;
mod completions;
mod config;
mod console;
mod doctor;
mod files;
mod fingerprint;
mod ignores;
mod image;
mod logs;
mod mutagen;
mod output;
mod presence;
mod project;
mod quote;
mod remote;
mod reset;
mod resolve;
mod secrets;
mod services;
mod setup;
mod shell;
mod snapshot;
mod ssh;
mod status;
mod tools;
mod tui;
mod update;
mod upgrade;
mod vps;
mod windows_terminal;

use std::env;

use agent::{Agent, Start};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Subcommand)]
enum Command {
    Setup {
        #[arg(long)]
        minimal: bool,
    },
    Attach {
        project: String,
        #[arg(default_value = ".")]
        dir_path: String,
        #[arg(long, value_enum)]
        agent: Option<Agent>,
        #[arg(long = "as")]
        alias: Option<String>,
    },
    Detach {
        project: String,
        #[arg(default_value = ".")]
        dir_path: String,
    },
    Chat {
        project: Option<String>,
        #[arg(long, value_enum)]
        agent: Option<Agent>,
        #[arg(long, conflicts_with = "open")]
        new: bool,
        #[arg(long)]
        open: Option<usize>,
    },
    Chats {
        project: Option<String>,
        #[arg(long, conflicts_with = "show")]
        rm: Option<usize>,
        #[arg(long)]
        show: Option<usize>,
    },
    Shell {
        project: Option<String>,
    },
    Vps,
    Change {
        #[arg(value_enum)]
        agent: Agent,
        project: Option<String>,
    },
    Resolve {
        project: Option<String>,
    },
    Files {
        project: Option<String>,
        folder: Option<String>,
    },
    History {
        project: Option<String>,
    },
    Services {
        project: Option<String>,
    },
    Snapshots {
        project: Option<String>,
    },
    Review {
        project: Option<String>,
    },
    Restore {
        snapshot: String,
        path: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    List,
    Status,
    Doctor,
    Logs,
    Update,
    Upgrade,
    Reset {
        #[arg(long)]
        all: bool,
    },
    Stop {
        project: Option<String>,
    },
    Rename {
        project: String,
        new_name: String,
    },
    Rm {
        project: String,
        #[arg(long)]
        purge: bool,
    },
    Completions {
        shell: Shell,
    },
    #[command(hide = true)]
    ImageTags,
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // An older sai asks this during `sai upgrade`: answer without logs, sync or completions
    if matches!(cli.command, Some(Command::ImageTags)) {
        image::print_tags();
        return Ok(());
    }
    console::restore_cursor_on_exit();
    let arguments = env::args().skip(1).collect::<Vec<_>>().join(" ");
    logs::start(if arguments.is_empty() {
        "tui"
    } else {
        &arguments
    });
    let interactive = matches!(
        cli.command,
        None | Some(Command::Chat { .. } | Command::Shell { .. } | Command::Vps)
    );
    let presence = presence::enter(interactive);
    let result = run(cli).await;
    console::show_cursor();
    if let Err(error) = &result {
        logs::error(error);
    }
    presence.leave();
    result
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    if !matches!(cli.command, Some(Command::Completions { .. })) {
        completions::install_quietly(Cli::command());
    }
    upgrade::clean_up();
    let Some(command) = cli.command else {
        return tui::run();
    };
    let remind = !matches!(command, Command::Upgrade | Command::Completions { .. });

    let result = match command {
        Command::Setup { minimal } => setup::run(minimal).await,
        Command::Attach {
            project,
            dir_path,
            agent,
            alias,
        } => {
            let alias = alias.as_deref().map(project::name_from).transpose()?;
            attach::run(
                &project::name_from(&project)?,
                &dir_path,
                agent,
                alias.as_deref(),
                &attach::ask_new_items,
                &attach::ask_git,
            )
        }
        Command::Detach { project, dir_path } => {
            project::detach::run_with_confirmation(&project, &dir_path)
        }
        Command::Chat {
            project,
            agent,
            new,
            open,
        } => match (new, open) {
            (_, Some(number)) => chat::open_number(project.as_deref(), agent, number),
            (true, None) => chat::run(project.as_deref(), agent, Start::New),
            (false, None) => chat::run(project.as_deref(), agent, Start::Ask),
        },
        Command::Chats { project, rm, show } => chat::list(project.as_deref(), rm, show),
        Command::Shell { project } => shell::run(project.as_deref()),
        Command::Vps => shell::vps(),
        Command::Change { agent, project } => project::change::run(project.as_deref(), agent),
        Command::Resolve { project } => resolve::run(project.as_deref()),
        Command::Files { project, folder } => {
            project::files::run(project.as_deref(), folder.as_deref())
        }
        Command::History { project } => project::history::run(project.as_deref()),
        Command::Services { project } => services::menu::run(project.as_deref()),
        Command::Review { project } => snapshot::review::run(project.as_deref()),
        Command::Snapshots { project } => snapshot::list(project.as_deref()),
        Command::Restore {
            snapshot,
            path,
            project,
        } => snapshot::restore_with_confirmation(project.as_deref(), &snapshot, path.as_deref()),
        Command::List => project::list::run(),
        Command::Status => status::run(),
        Command::Doctor => doctor::run(),
        Command::Logs => logs::report(),
        Command::Update => update::run(),
        Command::Upgrade => upgrade::run(),
        Command::Reset { all } => reset::run(all),
        Command::Stop { project } => project::stop::run(project.as_deref()),
        Command::Rename { project, new_name } => {
            project::rename::run(&project, &project::name_from(&new_name)?)
        }
        Command::Rm { project, purge } => project::remove::run_with_confirmation(&project, purge),
        Command::Completions { shell } => completions::print(shell, Cli::command()),
        Command::ImageTags => unreachable!("answered before anything starts"),
    };
    if remind {
        upgrade::notice();
    }
    result
}
