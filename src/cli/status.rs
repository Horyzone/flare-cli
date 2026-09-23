use anyhow::{bail, Context, Result};
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table};
use futures::future::join_all;

use crate::collector::Collector;
use crate::config::{ConfigManager, ServerConfig};
use super::args::StatusArgs;

pub async fn handle_status(args: StatusArgs) -> Result<()> {
    let config = ConfigManager::load()?;

    if config.servers.is_empty() {
        println!("{}", "No servers registered yet.".yellow());
        println!("Run {} to register a server.", "flare server add".cyan().bold());
        return Ok(());
    }

    match args.server_id {
        Some(ref id) => {
            let server = match config.find_server(id) {
                Some(s) => s,
                None => bail!("Server with ID '{}' not found in configuration", id),
            };
            inspect_single_server(server, args.json, args.timeout).await
        }
        None => {
            let servers = config.get_servers_filtered(args.tag.as_deref());
            if servers.is_empty() {
                println!("{}", "No servers matched the specified tag filter.".yellow());
                return Ok(());
            }
            inspect_all_servers(&servers, args.json, args.timeout).await
        }
    }
}

/// Detailed single-server inspection view.
async fn inspect_single_server(server: &ServerConfig, json_output: bool, timeout_secs: u64) -> Result<()> {
    if !json_output {
        println!(
            "{} Querying server {} ({}) metrics...",
            "•".cyan(),
            server.id.bold(),
            server.target_str().dimmed()
        );
    }

    let (metrics, duration_ms) = Collector::collect(server, timeout_secs).await
        .with_context(|| format!("Failed to collect metrics from server '{}'", server.id))?;

    if json_output {
        let json = serde_json::to_string_pretty(&metrics)?;
        println!("{}", json);
        return Ok(());
    }

    // Display formatted detailed dashboard
    println!();
    println!(
        "{} {}",
        "=== SERVER DASHBOARD:".bold().cyan(),
        format!("{} ({}) ===", server.name, server.id).bold().white()
    );
    println!("  {:<14}: {}", "Host".dimmed(), server.host_port_str());
    println!("  {:<14}: {}", "User".dimmed(), server.user);
    println!("  {:<14}: {}", "Remote Host".dimmed(), metrics.hostname.bold());
    println!("  {:<14}: {}", "Uptime".dimmed(), metrics.format_uptime().green());
    println!("  {:<14}: {} ms", "Probe Latency".dimmed(), duration_ms);
    println!();

    println!(
        "{} {:>5.1}%  {}",
        "CPU Load:".bold(),
        metrics.cpu_percent,
        render_progress_bar(metrics.cpu_percent, 24)
    );

    // Memory Section
    println!(
        "{} {:>5.1}%  {}  ({:.1} GiB / {:.1} GiB, Avail: {:.1} GiB)",
        "Memory:  ".bold(),
        metrics.memory.percent,
        render_progress_bar(metrics.memory.percent, 24),
        metrics.memory.used_gib(),
        metrics.memory.total_gib(),
        metrics.memory.available_gib()
    );

    // Disk Section
    println!(
        "{} {:>5.1}%  {}  ({:.1} GiB / {:.1} GiB, Avail: {:.1} GiB)",
        "Disk /:  ".bold(),
        metrics.disk.percent,
        render_progress_bar(metrics.disk.percent, 24),
        metrics.disk.used_gib(),
        metrics.disk.total_gib(),
        metrics.disk.available_gib()
    );
    println!();

    // GPU Section if present
    if !metrics.gpus.is_empty() {
        println!("{}", "GPU Metrics (NVIDIA):".bold().cyan());
        let mut gpu_table = Table::new();
        gpu_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("GPU MODEL").fg(Color::Cyan),
                Cell::new("UTIL %").fg(Color::Cyan),
                Cell::new("TEMP").fg(Color::Cyan),
                Cell::new("VRAM USED / TOTAL").fg(Color::Cyan),
            ]);

        for gpu in &metrics.gpus {
            gpu_table.add_row(vec![
                Cell::new(&gpu.name),
                Cell::new(format!("{:.1}%", gpu.utilization_percent)),
                Cell::new(format!("{:.0}°C", gpu.temperature_c)),
                Cell::new(format!("{:.0} MB / {:.0} MB", gpu.memory_used_mb, gpu.memory_total_mb)),
            ]);
        }
        println!("{gpu_table}");
        println!();
    }

    // Docker Section
    if metrics.docker.installed {
        println!(
            "{} Total: {} | Running: {} | Restarting: {} | Stopped: {}",
            "Docker Containers:".bold().cyan(),
            metrics.docker.total.to_string().bold(),
            metrics.docker.running.to_string().green().bold(),
            metrics.docker.restarting.to_string().color(if metrics.docker.restarting > 0 { "red" } else { "green" }).bold(),
            metrics.docker.stopped.to_string().color(if metrics.docker.stopped > 0 { "yellow" } else { "white" }).bold()
        );

        if !metrics.docker.containers.is_empty() {
            let mut container_table = Table::new();
            container_table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("ID").fg(Color::Cyan),
                    Cell::new("NAME").fg(Color::Cyan),
                    Cell::new("STATE").fg(Color::Cyan),
                    Cell::new("STATUS").fg(Color::Cyan),
                    Cell::new("IMAGE").fg(Color::Cyan),
                ]);

            for c in &metrics.docker.containers {
                let state_color = match c.state.as_str() {
                    "running" => Color::Green,
                    "restarting" => Color::Red,
                    _ => Color::Yellow,
                };

                container_table.add_row(vec![
                    Cell::new(c.id.chars().take(12).collect::<String>()),
                    Cell::new(&c.name).set_alignment(CellAlignment::Left),
                    Cell::new(&c.state).fg(state_color),
                    Cell::new(&c.status),
                    Cell::new(&c.image),
                ]);
            }
            println!("{container_table}");
        }
    } else {
        println!("{}", "Docker: Not installed or daemon not responding".yellow());
    }

    Ok(())
}

/// Synthesis multi-server parallel inspection view.
async fn inspect_all_servers(servers: &[&ServerConfig], json_output: bool, timeout_secs: u64) -> Result<()> {
    if !json_output {
        println!(
            "{} Gathering real-time metrics across {} server(s) in parallel...",
            "•".cyan(),
            servers.len()
        );
    }

    let futures = servers.iter().map(|s| async move {
        let res = Collector::collect(s, timeout_secs).await;
        (*s, res)
    });

    let results = join_all(futures).await;

    if json_output {
        let mut map = serde_json::Map::new();
        for (server, res) in results {
            match res {
                Ok((metrics, _)) => {
                    if let Ok(val) = serde_json::to_value(&metrics) {
                        map.insert(server.id.clone(), val);
                    }
                }
                Err(e) => {
                    map.insert(
                        server.id.clone(),
                        serde_json::json!({
                            "error": e.to_string(),
                            "reachable": false
                        }),
                    );
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&map)?);
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("SERVER").fg(Color::Cyan),
            Cell::new("HOST").fg(Color::Cyan),
            Cell::new("PROBE").fg(Color::Cyan),
            Cell::new("CPU %").fg(Color::Cyan),
            Cell::new("RAM % (USED/TOTAL)").fg(Color::Cyan),
            Cell::new("DISK % (USED/TOTAL)").fg(Color::Cyan),
            Cell::new("GPU").fg(Color::Cyan),
            Cell::new("DOCKER (RUN/REST/STOP)").fg(Color::Cyan),
            Cell::new("STATUS").fg(Color::Cyan),
        ]);

    for (server, res) in results {
        match res {
            Ok((metrics, duration_ms)) => {
                let cpu_cell = Cell::new(format!("{:.1}%", metrics.cpu_percent)).fg(
                    if metrics.cpu_percent >= 90.0 {
                        Color::Red
                    } else if metrics.cpu_percent >= 75.0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    },
                );

                let ram_cell = Cell::new(format!(
                    "{:.1}% ({:.1}/{:.1}G)",
                    metrics.memory.percent,
                    metrics.memory.used_gib(),
                    metrics.memory.total_gib()
                )).fg(if metrics.memory.percent >= 90.0 {
                    Color::Red
                } else if metrics.memory.percent >= 80.0 {
                    Color::Yellow
                } else {
                    Color::Green
                });

                let disk_cell = Cell::new(format!(
                    "{:.1}% ({:.0}/{:.0}G)",
                    metrics.disk.percent,
                    metrics.disk.used_gib(),
                    metrics.disk.total_gib()
                )).fg(if metrics.disk.percent >= 90.0 {
                    Color::Red
                } else if metrics.disk.percent >= 80.0 {
                    Color::Yellow
                } else {
                    Color::Green
                });

                let gpu_str = if metrics.gpus.is_empty() {
                    "-".to_string()
                } else {
                    format!("{:.0}% ({}°C)", metrics.gpus[0].utilization_percent, metrics.gpus[0].temperature_c)
                };

                let docker_cell = if metrics.docker.installed {
                    let text = format!("{}/{}/{}", metrics.docker.running, metrics.docker.restarting, metrics.docker.stopped);
                    if metrics.docker.restarting > 0 {
                        Cell::new(text).fg(Color::Red)
                    } else if metrics.docker.stopped > 0 {
                        Cell::new(text).fg(Color::Yellow)
                    } else {
                        Cell::new(text).fg(Color::Green)
                    }
                } else {
                    Cell::new("n/a").fg(Color::DarkGrey)
                };

                table.add_row(vec![
                    Cell::new(format!("{} ({})", server.name, server.id)),
                    Cell::new(server.host_port_str()),
                    Cell::new(format!("{}ms", duration_ms)),
                    cpu_cell,
                    ram_cell,
                    disk_cell,
                    Cell::new(gpu_str),
                    docker_cell,
                    Cell::new("● OK").fg(Color::Green),
                ]);
            }
            Err(_) => {
                table.add_row(vec![
                    Cell::new(format!("{} ({})", server.name, server.id)),
                    Cell::new(server.host_port_str()),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("✕ Unreachable").fg(Color::Red),
                ]);
            }
        }
    }

    println!("{table}");
    Ok(())
}

fn render_progress_bar(percentage: f64, width: usize) -> String {
    let clamped = percentage.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
    if clamped >= 90.0 {
        bar.red().to_string()
    } else if clamped >= 75.0 {
        bar.yellow().to_string()
    } else {
        bar.green().to_string()
    }
}
