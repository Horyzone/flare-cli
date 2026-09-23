use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Warning => "WARNING",
            AlertSeverity::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    HostUnreachable,
    HighCpu,
    HighRam,
    HighDisk,
    ContainerRestarting,
    ContainerStopped,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertType::HostUnreachable => "HOST_UNREACHABLE",
            AlertType::HighCpu => "HIGH_CPU",
            AlertType::HighRam => "HIGH_RAM",
            AlertType::HighDisk => "HIGH_DISK",
            AlertType::ContainerRestarting => "CONTAINER_RESTARTING",
            AlertType::ContainerStopped => "CONTAINER_STOPPED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub server_id: String,
    pub server_name: String,
    pub severity: AlertSeverity,
    pub alert_type: AlertType,
    pub target: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

impl Alert {
    pub fn new(
        server_id: impl Into<String>,
        server_name: impl Into<String>,
        severity: AlertSeverity,
        alert_type: AlertType,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            server_name: server_name.into(),
            severity,
            alert_type,
            target: target.into(),
            message: message.into(),
            timestamp: Utc::now(),
        }
    }

    /// Unique deduplication key for alert caching.
    pub fn cache_key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.server_id,
            self.alert_type.as_str(),
            self.target
        )
    }
}
