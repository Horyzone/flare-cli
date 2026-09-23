pub mod args;
pub mod check;
pub mod server;
pub mod ssh_cmd;
pub mod status;

pub use args::{Cli, Commands, ServerCommands};

use anyhow::Result;

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Server(sub) => match sub {
            ServerCommands::Add(args) => server::handle_add(args).await,
            ServerCommands::List(args) => server::handle_list(args).await,
            ServerCommands::Remove(args) => server::handle_remove(args).await,
        },
        Commands::Ssh(args) => ssh_cmd::handle_ssh(args),
        Commands::Status(args) => status::handle_status(args).await,
        Commands::Check(args) => check::handle_check(args).await,
    }
}
