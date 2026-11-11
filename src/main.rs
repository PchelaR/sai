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
mod project;
mod quote;
mod remote;
mod reset;
mod resolve;
mod secrets;
mod setup;
mod shell;
mod snapshot;
mod ssh;
mod status;
mod tools;
mod tui;
mod update;
mod vps;

use std::env;

use agent::Agent;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Subcommand)]
enum Command {
    Setup,
    Attach {
        #[arg(default_value = ".")]
        dir_path: String,
        #[arg(long, value_enum)]
        agent: Option<Agent>,
    },
    Chat {
        #[arg(default_value = ".")]
        dir_path: String,
        #[arg(long, value_enum)]
        agent: Option<Agent>,
    },
    Shell {
        #[arg(default_value = ".")]
        dir_path: String,
    },
    Change {
        #[arg(value_enum)]
        agent: Agent,
        #[arg(default_value = ".")]
        dir_path: String,
    },
    Resolve {
        #[arg(default_value = ".")]
        dir_path: String,
    },
    Snapshots {
        #[arg(default_value = ".")]
        dir_path: String,
    },
    Restore {
        snapshot: String,
        path: Option<String>,
        #[arg(long, default_value = ".")]
        dir: String,
    },
    List,
    Status,
    Doctor,
    Logs,
    Update,
    Reset {
        #[arg(long)]
        all: bool,
    },
    Stop {
        name: String,
    },
    Rm {
        name: String,
        #[arg(long)]
        purge: bool,
    },
    Completions {
        shell: Shell,
    },
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
    let arguments = env::args().skip(1).collect::<Vec<_>>().join(" ");
    logs::start(if arguments.is_empty() {
        "tui"
    } else {
        &arguments
    });
    let result = run(cli).await;
    if let Err(error) = &result {
        logs::error(error);
    }
    result
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let Some(command) = cli.command else {
        return tui::run();
    };

    match command {
        Command::Setup => setup::run().await,
        Command::Attach { dir_path, agent } => attach::run(&dir_path, agent),
        Command::Chat { dir_path, agent } => chat::run(&dir_path, agent),
        Command::Shell { dir_path } => shell::run(&dir_path),
        Command::Change { agent, dir_path } => project::change::run(&dir_path, agent),
        Command::Resolve { dir_path } => resolve::run(&dir_path),
        Command::Snapshots { dir_path } => snapshot::list(&dir_path),
        Command::Restore {
            snapshot,
            path,
            dir,
        } => snapshot::restore_with_confirmation(&dir, &snapshot, path.as_deref()),
        Command::List => project::list::run(),
        Command::Status => status::run(),
        Command::Doctor => doctor::run(),
        Command::Logs => logs::report(),
        Command::Update => update::run(),
        Command::Reset { all } => reset::run(all),
        Command::Stop { name } => project::stop::run(&name),
        Command::Rm { name, purge } => project::remove::run(&name, purge),
        Command::Completions { shell } => completions::print(shell, Cli::command()),
    }
}
