mod agent;
mod attach;
mod chat;
mod completions;
mod config;
mod doctor;
mod files;
mod fingerprint;
mod ignores;
mod image;
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
mod update;
mod vps;

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
    command: Command,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
        } => snapshot::restore(&dir, &snapshot, path.as_deref()),
        Command::List => project::list::run(),
        Command::Status => status::run(),
        Command::Doctor => doctor::run(),
        Command::Update => update::run(),
        Command::Reset { all } => reset::run(all),
        Command::Stop { name } => project::stop::run(&name),
        Command::Rm { name, purge } => project::remove::run(&name, purge),
        Command::Completions { shell } => completions::print(shell, Cli::command()),
    }
}
