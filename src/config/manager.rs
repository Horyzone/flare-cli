use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::models::{AlertsConfig, AppConfig, ServerConfig};

pub const CONFIG_DIR_NAME: &str = "flare";
pub const CONFIG_FILE_NAME: &str = "config.yaml";

pub struct ConfigManager;

impl ConfigManager {
    /// Returns the path to the configuration directory: `~/.config/flare/`
    pub fn config_dir() -> Result<PathBuf> {
        let base_dir = dirs::config_dir().context("Could not determine user config directory")?;
        Ok(base_dir.join(CONFIG_DIR_NAME))
    }

    /// Returns the path to `~/.config/flare/config.yaml`
    pub fn config_file_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE_NAME))
    }

    /// Expands `~` or returns absolute/relative path.
    pub fn expand_path(raw: &str) -> PathBuf {
        if let Some(stripped) = raw.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(raw)
    }

    /// Loads the configuration file, creating a default one if it doesn't exist.
    pub fn load() -> Result<AppConfig> {
        let path = Self::config_file_path()?;

        if !path.exists() {
            let default_config = Self::create_default_config_file(&path)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file at {}", path.display()))?;

        let config: AppConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML config from {}", path.display()))?;

        Ok(config)
    }

    /// Saves the current configuration to disk.
    pub fn save(config: &AppConfig) -> Result<()> {
        let path = Self::config_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir {}", parent.display()))?;
        }

        let yaml = serde_yaml::to_string(config)
            .context("Failed to serialize configuration to YAML")?;

        fs::write(&path, yaml)
            .with_context(|| format!("Failed to write config file to {}", path.display()))?;

        Ok(())
    }

    /// Creates an initial configuration file with helpful comments and an example server.
    fn create_default_config_file(path: &Path) -> Result<AppConfig> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir {}", parent.display()))?;
        }

        let default_config = AppConfig {
            servers: vec![ServerConfig {
                id: "example-node".to_string(),
                name: "Example VPS / Dokploy".to_string(),
                host: "127.0.0.1".to_string(),
                port: 22,
                user: "root".to_string(),
                key_path: Some("~/.ssh/id_ed25519".to_string()),
                tags: vec!["dokploy".to_string(), "demo".to_string()],
            }],
            alerts: AlertsConfig {
                webhook_url: Some("https://ntfy.sh/flare_alerts_example".to_string()),
                cpu_percent: 85.0,
                ram_percent: 90.0,
                disk_percent: 85.0,
                notify_stopped_containers: true,
                notify_restarting_containers: true,
                cooldown_minutes: 60,
            },
        };

        let yaml = serde_yaml::to_string(&default_config)
            .context("Failed to serialize default config to YAML")?;

        let header = "# Flare CLI Configuration\n# Store servers and alert settings here.\n\n";
        let full_content = format!("{}{}", header, yaml);

        fs::write(path, full_content)
            .with_context(|| format!("Failed to write initial config file at {}", path.display()))?;

        Ok(default_config)
    }
}

impl AppConfig {
    /// Finds a server by its slug ID.
    pub fn find_server(&self, id: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// Adds a server to the configuration. Fails if ID already exists.
    pub fn add_server(&mut self, server: ServerConfig) -> Result<()> {
        if self.find_server(&server.id).is_some() {
            bail!("A server with ID '{}' already exists", server.id);
        }
        self.servers.push(server);
        Ok(())
    }

    /// Removes a server by ID. Returns the removed server.
    pub fn remove_server(&mut self, id: &str) -> Result<ServerConfig> {
        if let Some(index) = self.servers.iter().position(|s| s.id.eq_ignore_ascii_case(id)) {
            Ok(self.servers.remove(index))
        } else {
            bail!("No server found with ID '{}'", id);
        }
    }

    /// Filters servers by tag (if tags are provided).
    pub fn get_servers_filtered(&self, tag: Option<&str>) -> Vec<&ServerConfig> {
        match tag {
            Some(t) => self
                .servers
                .iter()
                .filter(|s| s.tags.iter().any(|st| st.eq_ignore_ascii_case(t)))
                .collect(),
            None => self.servers.iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_remove_server() {
        let mut config = AppConfig::default();
        let server = ServerConfig {
            id: "dokploy-prod".to_string(),
            name: "Dokploy Production".to_string(),
            host: "1.2.3.4".to_string(),
            port: 2222,
            user: "admin".to_string(),
            key_path: Some("~/.ssh/id_rsa".to_string()),
            tags: vec!["prod".to_string(), "dokploy".to_string()],
        };

        assert!(config.add_server(server.clone()).is_ok());
        // Duplicate ID should fail
        assert!(config.add_server(server).is_err());

        assert!(config.find_server("dokploy-prod").is_some());
        assert_eq!(config.find_server("DOKPLOY-PROD").unwrap().port, 2222);

        let filtered_prod = config.get_servers_filtered(Some("prod"));
        assert_eq!(filtered_prod.len(), 1);

        let filtered_dev = config.get_servers_filtered(Some("dev"));
        assert_eq!(filtered_dev.len(), 0);

        let removed = config.remove_server("dokploy-prod");
        assert!(removed.is_ok());
        assert_eq!(removed.unwrap().id, "dokploy-prod");
        assert!(config.find_server("dokploy-prod").is_none());
    }

    #[test]
    fn test_expand_path() {
        let path = ConfigManager::expand_path("~/my_key.pem");
        assert!(!path.to_string_lossy().starts_with("~/"));
    }
}
