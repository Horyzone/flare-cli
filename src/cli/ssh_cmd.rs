use anyhow::{bail, Result};
use colored::Colorize;
use inquire::Select;

use crate::config::ConfigManager;
use crate::ssh::SshClient;
use super::args::SshArgs;

pub fn handle_ssh(args: SshArgs) -> Result<()> {
    let config = ConfigManager::load()?;

    if config.servers.is_empty() {
        println!("{}", "No servers registered yet.".yellow());
        println!("Run {} to add a server first.", "flare server add".cyan().bold());
        return Ok(());
    }

    let target_server = match args.server_id {
        Some(id) => match config.find_server(&id) {
            Some(s) => s.clone(),
            None => bail!("Server with ID '{}' not found in configuration", id),
        },
        None => {
            let options: Vec<String> = config
                .servers
                .iter()
                .map(|s| format!("{} - {} ({})", s.id, s.name, s.target_str()))
                .collect();

            let selection = Select::new("Select server to SSH into:", options).prompt()?;
            let selected_id = selection
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim();

            match config.find_server(selected_id) {
                Some(s) => s.clone(),
                None => bail!("Selected server '{}' not found", selected_id),
            }
        }
    };

    println!(
        "{} Connecting to {} ({}) via SSH...",
        "⚡".cyan(),
        target_server.name.bold(),
        target_server.target_str().dimmed()
    );

    SshClient::open_interactive_shell(&target_server)
}
