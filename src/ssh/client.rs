use anyhow::{bail, Context, Result};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

use crate::config::{ConfigManager, ServerConfig};

pub struct SshClient;

impl SshClient {
    /// Builds standard arguments for the `ssh` command based on server configuration.
    pub fn build_ssh_args(server: &ServerConfig, interactive: bool) -> Vec<String> {
        let mut args = Vec::new();

        // Custom port
        args.push("-p".to_string());
        args.push(server.port.to_string());

        // Custom key path if provided
        if let Some(ref key_path) = server.key_path {
            let expanded = ConfigManager::expand_path(key_path);
            args.push("-i".to_string());
            args.push(expanded.to_string_lossy().to_string());
        }

        if interactive {
            // Force pseudo-terminal allocation for interactive shell sessions
            args.push("-t".to_string());
        } else {
            // Non-interactive options for automated scripting
            args.push("-o".to_string());
            args.push("BatchMode=yes".to_string());
            args.push("-o".to_string());
            args.push("ConnectTimeout=6".to_string());
            args.push("-o".to_string());
            args.push("StrictHostKeyChecking=accept-new".to_string());
            args.push("-o".to_string());
            args.push("LogLevel=ERROR".to_string());
        }

        // Target: user@host
        args.push(format!("{}@{}", server.user, server.host));

        args
    }

    /// Fast TCP reachability check with latency measurement.
    pub async fn check_tcp(host: &str, port: u16, timeout_duration: Duration) -> Result<Duration> {
        let start = Instant::now();
        let addr = format!("{}:{}", host, port);

        match timeout(timeout_duration, TcpStream::connect(&addr)).await {
            Ok(Ok(_stream)) => Ok(start.elapsed()),
            Ok(Err(e)) => bail!("TCP connection to {} failed: {}", addr, e),
            Err(_) => bail!("TCP connection to {} timed out after {:?}", addr, timeout_duration),
        }
    }

    /// Checks full SSH reachability & authentication.
    pub async fn check_connection(server: &ServerConfig) -> Result<Duration> {
        // Step 1: Rapid TCP check (2s timeout)
        let _ = Self::check_tcp(&server.host, server.port, Duration::from_secs(2)).await
            .with_context(|| format!("Host {}:{} is not reachable on TCP", server.host, server.port))?;

        // Step 2: SSH authentication probe
        let start = Instant::now();
        let mut cmd = TokioCommand::new("ssh");
        let args = Self::build_ssh_args(server, false);
        cmd.args(&args);
        cmd.arg("true");

        let probe_timeout = Duration::from_secs(6);
        let output = timeout(probe_timeout, cmd.output())
            .await
            .map_err(|_| anyhow::anyhow!("SSH connection to {} timed out after 6s", server.id))?
            .with_context(|| format!("Failed to execute local ssh binary for server {}", server.id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "SSH authentication/handshake failed for {} ({}): {}",
                server.id,
                server.target_str(),
                if stderr.is_empty() { "Authentication failed or host refused key" } else { &stderr }
            );
        }

        Ok(start.elapsed())
    }

    /// Runs a command remotely over SSH asynchronously and returns stdout.
    pub async fn run_command(server: &ServerConfig, command_str: &str, timeout_secs: u64) -> Result<String> {
        let mut cmd = TokioCommand::new("ssh");
        let args = Self::build_ssh_args(server, false);
        cmd.args(&args);
        cmd.arg(command_str);

        // Disconnect stdin to prevent remote process from hanging
        cmd.stdin(Stdio::null());

        let time_limit = Duration::from_secs(timeout_secs);
        let output = timeout(time_limit, cmd.output())
            .await
            .map_err(|_| anyhow::anyhow!("Command execution on {} timed out after {}s", server.id, timeout_secs))?
            .with_context(|| format!("Failed to invoke ssh binary for {}", server.id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let details = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("Process exited with status code {:?}", output.status.code())
            };
            bail!("Remote execution on '{}' failed: {}", server.id, details);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Spawns an interactive SSH session attached to the current terminal (inherits stdin/stdout/stderr).
    pub fn open_interactive_shell(server: &ServerConfig) -> Result<()> {
        let mut cmd = std::process::Command::new("ssh");
        let args = Self::build_ssh_args(server, true);
        cmd.args(&args);

        // Replace current process or run directly inheriting terminal
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Under Unix, we can cleanly run status so signals and exit are passed back
        let status = cmd.status().with_context(|| {
            format!("Failed to start interactive SSH session to {}", server.id)
        })?;

        if !status.success() {
            if let Some(code) = status.code() {
                if code != 0 && code != 130 { // 130 is Ctrl+C
                    eprintln!("SSH session exited with code {}", code);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ssh_args_default() {
        let server = ServerConfig {
            id: "my-srv".to_string(),
            name: "My Server".to_string(),
            host: "server.example.com".to_string(),
            port: 22,
            user: "root".to_string(),
            key_path: None,
            tags: vec![],
        };

        let non_interactive_args = SshClient::build_ssh_args(&server, false);
        assert!(non_interactive_args.contains(&"-p".to_string()));
        assert!(non_interactive_args.contains(&"22".to_string()));
        assert!(non_interactive_args.contains(&"BatchMode=yes".to_string()));
        assert_eq!(non_interactive_args.last().unwrap(), "root@server.example.com");

        let interactive_args = SshClient::build_ssh_args(&server, true);
        assert!(interactive_args.contains(&"-t".to_string()));
        assert!(!interactive_args.contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn test_build_ssh_args_custom_key_and_port() {
        let server = ServerConfig {
            id: "my-srv".to_string(),
            name: "My Server".to_string(),
            host: "1.2.3.4".to_string(),
            port: 2222,
            user: "deploy".to_string(),
            key_path: Some("/custom/key".to_string()),
            tags: vec![],
        };

        let args = SshClient::build_ssh_args(&server, false);
        assert!(args.contains(&"2222".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/custom/key".to_string()));
        assert_eq!(args.last().unwrap(), "deploy@1.2.3.4");
    }
}
