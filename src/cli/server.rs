use anyhow::{bail, Result};
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table};
use futures::future::join_all;
use inquire::{Confirm, CustomType, Select, Text};

use super::args::{ServerAddArgs, ServerListArgs, ServerRemoveArgs};
use crate::config::{ConfigManager, ServerConfig};
use crate::ssh::SshClient;

pub async fn handle_add(args: ServerAddArgs) -> Result<()> {
    let mut config = ConfigManager::load()?;

    let id = match args.id {
        Some(val) => val.trim().to_string(),
        None => Text::new("Server ID (slug):")
            .with_placeholder("prod-vps-1")
            .with_help_message("Unique slug for this server")
            .prompt()?,
    };

    if id.is_empty() {
        bail!("Server ID cannot be empty");
    }

    if config.find_server(&id).is_some() {
        bail!("A server with ID '{}' already exists in the inventory", id);
    }

    let name = match args.name {
        Some(val) => val.trim().to_string(),
        None => Text::new("Display name:").with_default(&id).prompt()?,
    };

    let host = match args.host {
        Some(val) => val.trim().to_string(),
        None => Text::new("Host / IP:")
            .with_placeholder("192.168.1.100 or myserver.example.com")
            .prompt()?,
    };

    if host.is_empty() {
        bail!("Host cannot be empty");
    }

    let port = match args.port {
        Some(p) => p,
        None => CustomType::<u16>::new("SSH port:")
            .with_default(22)
            .prompt()?,
    };

    let user = match args.user {
        Some(u) => u.trim().to_string(),
        None => Text::new("SSH user:").with_default("root").prompt()?,
    };

    let key_path = match args.key_path {
        Some(k) => Some(k.trim().to_string()),
        None => {
            let key_input =
                Text::new("Custom SSH key path (optional, leave empty for default agent/config):")
                    .with_placeholder("~/.ssh/id_ed25519")
                    .prompt()?;
            let trimmed = key_input.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
    };

    let tags = match args.tags {
        Some(t) => t,
        None => {
            let tags_str = Text::new("Tags (comma separated, optional):")
                .with_placeholder("prod, dokploy")
                .prompt()?;
            tags_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
    };

    let new_server = ServerConfig {
        id: id.clone(),
        name,
        host,
        port,
        user,
        key_path,
        tags,
    };

    if !args.skip_test {
        println!(
            "{}",
            format!("Testing SSH connectivity to '{}'...", new_server.id).cyan()
        );
        match SshClient::check_connection(&new_server).await {
            Ok(duration) => {
                println!(
                    "{} Connection verified successfully in {}ms!",
                    "✔".green().bold(),
                    duration.as_millis()
                );
            }
            Err(e) => {
                eprintln!("{} Connection test failed: {}", "✖".red().bold(), e);
                let proceed = Confirm::new("Do you still want to save this server?")
                    .with_default(false)
                    .prompt()?;
                if !proceed {
                    println!("{}", "Operation cancelled.".yellow());
                    return Ok(());
                }
            }
        }
    }

    config.add_server(new_server)?;
    ConfigManager::save(&config)?;

    println!(
        "{} Server '{}' successfully added to {}",
        "✔".green().bold(),
        id.bold(),
        ConfigManager::config_file_path()?
            .display()
            .to_string()
            .underline()
    );

    Ok(())
}

pub async fn handle_list(args: ServerListArgs) -> Result<()> {
    let config = ConfigManager::load()?;
    let servers = config.get_servers_filtered(args.tag.as_deref());

    if servers.is_empty() {
        if config.servers.is_empty() {
            println!(
                "{}",
                "No servers registered yet in ~/.config/flare/config.yaml".yellow()
            );
            println!(
                "Run {} to register a server.",
                "flare server add".cyan().bold()
            );
        } else {
            println!("{}", "No servers match the specified tag filter.".yellow());
        }
        return Ok(());
    }

    println!(
        "{} Checking availability for {} server(s)...",
        "•".cyan(),
        servers.len()
    );

    let check_futures = servers.iter().map(|s| async move {
        let res = SshClient::check_connection(s).await;
        (s, res)
    });

    let results = join_all(check_futures).await;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("NAME").fg(Color::Cyan),
            Cell::new("HOST:PORT").fg(Color::Cyan),
            Cell::new("USER").fg(Color::Cyan),
            Cell::new("TAGS").fg(Color::Cyan),
            Cell::new("STATUS").fg(Color::Cyan),
        ]);

    for (server, res) in results {
        let (status_text, color) = match res {
            Ok(lat) => (format!("● Online ({}ms)", lat.as_millis()), Color::Green),
            Err(e) => {
                let err_msg = e.to_string();
                let short_err = if err_msg.contains("timed out") {
                    "✕ Timeout"
                } else if err_msg.contains("TCP") {
                    "✕ TCP Unreachable"
                } else {
                    "✕ Auth/SSH Failed"
                };
                (short_err.to_string(), Color::Red)
            }
        };

        let tags_str = if server.tags.is_empty() {
            "-".to_string()
        } else {
            server.tags.join(", ")
        };

        table.add_row(vec![
            Cell::new(&server.id).set_alignment(CellAlignment::Left),
            Cell::new(&server.name),
            Cell::new(server.host_port_str()),
            Cell::new(&server.user),
            Cell::new(tags_str),
            Cell::new(status_text).fg(color),
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn handle_remove(args: ServerRemoveArgs) -> Result<()> {
    let mut config = ConfigManager::load()?;

    if config.servers.is_empty() {
        println!("{}", "No servers registered in configuration.".yellow());
        return Ok(());
    }

    let server_id = match args.id {
        Some(id) => id,
        None => {
            let options: Vec<String> = config
                .servers
                .iter()
                .map(|s| format!("{} ({}) - {}", s.id, s.name, s.host))
                .collect();

            let selection = Select::new("Select server to remove:", options).prompt()?;
            // Extract the ID before the first space
            selection
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string()
        }
    };

    if !args.force {
        let confirm = Confirm::new(&format!(
            "Are you sure you want to remove server '{}'?",
            server_id
        ))
        .with_default(false)
        .prompt()?;
        if !confirm {
            println!("{}", "Operation cancelled.".yellow());
            return Ok(());
        }
    }

    let removed = config.remove_server(&server_id)?;
    ConfigManager::save(&config)?;

    println!(
        "{} Server '{}' ({}) has been removed from inventory.",
        "✔".green().bold(),
        removed.id.bold(),
        removed.name
    );

    Ok(())
}
