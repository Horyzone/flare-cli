use clap::Parser;
use colored::Colorize;
use std::process::exit;

mod alerts;
mod cli;
mod collector;
mod config;
mod ssh;

use cli::{Cli, run};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(err) = run(cli).await {
        eprintln!("{} {}", "Error:".red().bold(), err);
        // Print error chain if available
        let mut source = err.source();
        while let Some(cause) = source {
            eprintln!("  {} {}", "Caused by:".dimmed(), cause);
            source = cause.source();
        }
        exit(1);
    }
}
