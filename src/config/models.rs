use serde::{Deserialize, Serialize};

fn default_ssh_port() -> u16 {
    22
}

fn default_ssh_user() -> String {
    "root".to_string()
}

fn default_cpu_threshold() -> f64 {
    85.0
}

fn default_ram_threshold() -> f64 {
    90.0
}

fn default_disk_threshold() -> f64 {
    85.0
}

fn default_notify_stopped() -> bool {
    true
}

fn default_notify_restarting() -> bool {
    true
}

fn default_cooldown_minutes() -> u64 {
    60
}

/// Representation of a remote managed server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    /// Unique slug ID (e.g. "prod-dokploy-1")
    pub id: String,

    /// Human-friendly display name
    pub name: String,

    /// Hostname or IP address
    pub host: String,

    /// SSH port (defaults to 22)
    #[serde(default = "default_ssh_port")]
    pub port: u16,

    /// SSH username (defaults to "root")
    #[serde(default = "default_ssh_user")]
    pub user: String,

    /// Optional path to custom SSH private key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,

    /// Tags for grouping/filtering (e.g. ["prod", "dokploy"])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl ServerConfig {
    /// Formats the target as `user@host` or `user@host:port`.
    pub fn target_str(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    /// Formats the host and port (e.g. `192.168.1.10:22`).
    pub fn host_port_str(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Alerting configuration and thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertsConfig {
    /// Optional webhook URL (e.g. ntfy.sh topic or generic webhook)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,

    /// CPU usage threshold in percentage (0-100)
    #[serde(default = "default_cpu_threshold")]
    pub cpu_percent: f64,

    /// RAM usage threshold in percentage (0-100)
    #[serde(default = "default_ram_threshold")]
    pub ram_percent: f64,

    /// Root disk usage threshold in percentage (0-100)
    #[serde(default = "default_disk_threshold")]
    pub disk_percent: f64,

    /// Whether to trigger an alert if non-running containers are detected
    #[serde(default = "default_notify_stopped")]
    pub notify_stopped_containers: bool,

    /// Whether to trigger an alert if restarting (crash loop) containers are detected
    #[serde(default = "default_notify_restarting")]
    pub notify_restarting_containers: bool,

    /// Cooldown window in minutes before repeating the same alert
    #[serde(default = "default_cooldown_minutes")]
    pub cooldown_minutes: u64,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            webhook_url: None,
            cpu_percent: default_cpu_threshold(),
            ram_percent: default_ram_threshold(),
            disk_percent: default_disk_threshold(),
            notify_stopped_containers: default_notify_stopped(),
            notify_restarting_containers: default_notify_restarting(),
            cooldown_minutes: default_cooldown_minutes(),
        }
    }
}

/// Root configuration file content.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AppConfig {
    /// List of registered servers
    #[serde(default)]
    pub servers: Vec<ServerConfig>,

    /// Alerting configuration
    #[serde(default)]
    pub alerts: AlertsConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let yaml = r#"
id: "test-node"
name: "Test Node"
host: "10.0.0.1"
"#;
        let server: ServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(server.id, "test-node");
        assert_eq!(server.port, 22);
        assert_eq!(server.user, "root");
        assert_eq!(server.key_path, None);
        assert!(server.tags.is_empty());
        assert_eq!(server.target_str(), "root@10.0.0.1");
        assert_eq!(server.host_port_str(), "10.0.0.1:22");
    }

    #[test]
    fn test_alerts_config_defaults() {
        let default_alerts = AlertsConfig::default();
        assert_eq!(default_alerts.cpu_percent, 85.0);
        assert_eq!(default_alerts.ram_percent, 90.0);
        assert_eq!(default_alerts.disk_percent, 85.0);
        assert!(default_alerts.notify_stopped_containers);
        assert!(default_alerts.notify_restarting_containers);
        assert_eq!(default_alerts.cooldown_minutes, 60);
    }
}
