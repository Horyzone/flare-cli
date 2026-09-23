use anyhow::Result;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};
use futures::future::join_all;

use crate::alerts::{Alert, AlertCache, AlertChecker, AlertSeverity, Notifier};
use crate::collector::Collector;
use crate::config::ConfigManager;
use super::args::CheckArgs;

pub async fn handle_check(args: CheckArgs) -> Result<()> {
    let config = ConfigManager::load()?;

    let servers = config.get_servers_filtered(args.tag.as_deref());
    if servers.is_empty() {
        println!("{}", "No servers registered or matching the filter.".yellow());
        return Ok(());
    }

    println!(
        "{} Running sentinel health check on {} server(s)...",
        "🛡".cyan(),
        servers.len()
    );

    let futures = servers.iter().map(|s| async move {
        let res = Collector::collect(s, args.timeout).await;
        (*s, res)
    });

    let results = join_all(futures).await;

    // Collect all evaluated alerts
    let mut all_alerts: Vec<Alert> = Vec::new();

    for (server, res) in results {
        let alerts = match res {
            Ok((metrics, _)) => AlertChecker::evaluate(server, Ok(&metrics), &config.alerts),
            Err(e) => {
                let err_msg = e.to_string();
                AlertChecker::evaluate(server, Err(&err_msg), &config.alerts)
            }
        };
        all_alerts.extend(alerts);
    }

    if all_alerts.is_empty() {
        println!(
            "{} All {} server(s) are healthy! No alert conditions met.",
            "✔".green().bold(),
            servers.len()
        );
        return Ok(());
    }

    // Process alerts through cache
    let mut cache = AlertCache::load();
    let mut alerts_to_notify = Vec::new();
    let mut suppressed_alerts = Vec::new();

    for alert in all_alerts {
        if args.force || cache.should_notify(&alert, config.alerts.cooldown_minutes) {
            alerts_to_notify.push(alert);
        } else {
            suppressed_alerts.push(alert);
        }
    }

    // Render alert overview table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("SEVERITY").fg(Color::Cyan),
            Cell::new("SERVER").fg(Color::Cyan),
            Cell::new("TYPE").fg(Color::Cyan),
            Cell::new("TARGET").fg(Color::Cyan),
            Cell::new("MESSAGE").fg(Color::Cyan),
            Cell::new("ACTION").fg(Color::Cyan),
        ]);

    for a in &alerts_to_notify {
        let (sev_cell, col) = match a.severity {
            AlertSeverity::Critical => ("CRITICAL", Color::Red),
            AlertSeverity::Warning => ("WARNING", Color::Yellow),
        };
        table.add_row(vec![
            Cell::new(sev_cell).fg(col),
            Cell::new(&a.server_name),
            Cell::new(a.alert_type.as_str()),
            Cell::new(&a.target),
            Cell::new(&a.message),
            Cell::new(if args.dry_run { "Dry Run (Skip)" } else { "Send Webhook" }).fg(Color::Cyan),
        ]);
    }

    for a in &suppressed_alerts {
        table.add_row(vec![
            Cell::new(a.severity.as_str()).fg(Color::DarkGrey),
            Cell::new(&a.server_name),
            Cell::new(a.alert_type.as_str()),
            Cell::new(&a.target),
            Cell::new(&a.message),
            Cell::new("Suppressed (Cooldown)").fg(Color::DarkGrey),
        ]);
    }

    println!("{table}");
    println!();

    if alerts_to_notify.is_empty() {
        println!(
            "{}",
            "Active alerts exist but were suppressed by the local cooldown cache. Use --force to bypass."
                .yellow()
        );
        return Ok(());
    }

    if args.dry_run {
        println!(
            "{} Dry-run mode: {} alert(s) would be dispatched.",
            "ℹ".cyan(),
            alerts_to_notify.len()
        );
        return Ok(());
    }

    // Dispatch webhook
    match config.alerts.webhook_url.as_deref() {
        Some(url) if !url.trim().is_empty() => {
            print!("Dispatching {} alert(s) to webhook... ", alerts_to_notify.len());
            match Notifier::dispatch(url, &alerts_to_notify).await {
                Ok(_) => {
                    println!("{}", "Sent successfully!".green().bold());
                    // Record in cache
                    for alert in &alerts_to_notify {
                        cache.record_notification(alert);
                    }
                    if let Err(e) = cache.save() {
                        eprintln!("Warning: Failed to save alert cache: {}", e);
                    }
                }
                Err(e) => {
                    println!("{}", "Failed!".red().bold());
                    eprintln!("Error dispatching alert: {}", e);
                }
            }
        }
        _ => {
            println!(
                "{} Alert conditions triggered, but no webhook URL is configured.",
                "⚠".yellow().bold()
            );
            println!(
                "Configure {} in {} to receive notifications.",
                "alerts.webhook_url".bold(),
                ConfigManager::config_file_path()?.display().to_string().underline()
            );
        }
    }

    Ok(())
}
