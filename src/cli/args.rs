use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "flare",
    author = "Simon Micheneau <contact@simon-micheneau.fr>",
    version,
    about = "Agentless CLI administration & monitoring for Docker / Dokploy remote servers",
    long_about = "Flare CLI is an agentless DevOps operations and monitoring tool for remote Linux servers running Docker and Dokploy.\nIt leverages native SSH connections, zero-dependency remote probes, and automated alerting."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage remote servers in the local inventory
    #[command(subcommand)]
    Server(ServerCommands),

    /// Open an interactive SSH shell to a managed server
    Ssh(SshArgs),

    /// Collect and display real-time metrics (CPU, RAM, Disk, GPU, Docker)
    Status(StatusArgs),

    /// Sentinel health check & alerting (designed for cron or manual triggers)
    Check(CheckArgs),
}

#[derive(Subcommand, Debug)]
pub enum ServerCommands {
    /// Add a new server to the inventory (interactive or with flags)
    Add(ServerAddArgs),

    /// List all registered servers with live reachability status
    List(ServerListArgs),

    /// Remove a server from the inventory
    Remove(ServerRemoveArgs),
}

#[derive(Args, Debug)]
pub struct ServerAddArgs {
    /// Unique server ID slug (e.g. prod-dokploy-1)
    #[arg(short, long)]
    pub id: Option<String>,

    /// Human-readable display name
    #[arg(short, long)]
    pub name: Option<String>,

    /// Hostname or IP address
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// SSH port
    #[arg(short, long)]
    pub port: Option<u16>,

    /// SSH username
    #[arg(short, long)]
    pub user: Option<String>,

    /// Path to private SSH key (e.g. ~/.ssh/id_ed25519)
    #[arg(short, long)]
    pub key_path: Option<String>,

    /// Tags for grouping (comma-separated, e.g. "prod,dokploy")
    #[arg(short, long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,

    /// Skip connection test before adding
    #[arg(long)]
    pub skip_test: bool,
}

#[derive(Args, Debug)]
pub struct ServerListArgs {
    /// Filter servers by tag
    #[arg(short, long)]
    pub tag: Option<String>,
}

#[derive(Args, Debug)]
pub struct ServerRemoveArgs {
    /// Server ID to remove (prompts interactively if omitted)
    pub id: Option<String>,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct SshArgs {
    /// Server ID to connect to (prompts interactively if omitted)
    pub server_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Server ID to inspect (if omitted, summarizes all servers in parallel)
    pub server_id: Option<String>,

    /// Output full raw JSON data instead of table
    #[arg(long)]
    pub json: bool,

    /// Filter servers by tag (when inspecting all servers)
    #[arg(short, long)]
    pub tag: Option<String>,

    /// SSH execution timeout in seconds
    #[arg(long, default_value_t = 12)]
    pub timeout: u64,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Run checks without dispatching actual webhook alerts
    #[arg(long)]
    pub dry_run: bool,

    /// Bypass cooldown cache and force notifications
    #[arg(long)]
    pub force: bool,

    /// Filter servers by tag
    #[arg(short, long)]
    pub tag: Option<String>,

    /// SSH execution timeout in seconds
    #[arg(long, default_value_t = 12)]
    pub timeout: u64,
}
