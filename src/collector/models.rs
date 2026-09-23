use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetrics {
    pub hostname: String,
    pub uptime_seconds: Option<f64>,
    pub cpu_percent: f64,
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    #[serde(default)]
    pub gpus: Vec<GpuMetrics>,
    #[serde(default)]
    pub docker: DockerMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub percent: f64,
}

impl MemoryMetrics {
    pub fn total_gib(&self) -> f64 {
        self.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn used_gib(&self) -> f64 {
        self.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn available_gib(&self) -> f64 {
        self.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub percent: f64,
}

impl DiskMetrics {
    pub fn total_gib(&self) -> f64 {
        self.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn used_gib(&self) -> f64 {
        self.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn available_gib(&self) -> f64 {
        self.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub name: String,
    pub utilization_percent: f64,
    pub temperature_c: f64,
    pub memory_total_mb: f64,
    pub memory_used_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockerMetrics {
    pub installed: bool,
    pub running: usize,
    pub restarting: usize,
    pub stopped: usize,
    pub total: usize,
    #[serde(default)]
    pub containers: Vec<ContainerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub status: String,
    pub image: String,
}

impl ServerMetrics {
    /// Human readable uptime formatted as `Xd Yh Zm`.
    pub fn format_uptime(&self) -> String {
        match self.uptime_seconds {
            Some(secs) => {
                let s = secs as u64;
                let days = s / 86400;
                let hours = (s % 86400) / 3600;
                let minutes = (s % 3600) / 60;
                if days > 0 {
                    format!("{}d {}h {}m", days, hours, minutes)
                } else if hours > 0 {
                    format!("{}h {}m", hours, minutes)
                } else {
                    format!("{}m", minutes)
                }
            }
            None => "unknown".to_string(),
        }
    }
}
