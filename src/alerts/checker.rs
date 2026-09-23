use super::models::{Alert, AlertSeverity, AlertType};
use crate::collector::ServerMetrics;
use crate::config::{AlertsConfig, ServerConfig};

pub struct AlertChecker;

impl AlertChecker {
    /// Evaluates server status and metrics against the configured alert thresholds.
    pub fn evaluate(
        server: &ServerConfig,
        metrics_result: Result<&ServerMetrics, &str>,
        config: &AlertsConfig,
    ) -> Vec<Alert> {
        let mut alerts = Vec::new();

        match metrics_result {
            Err(err_msg) => {
                alerts.push(Alert::new(
                    &server.id,
                    &server.name,
                    AlertSeverity::Critical,
                    AlertType::HostUnreachable,
                    "host",
                    format!(
                        "Server is unreachable or SSH connection failed: {}",
                        err_msg
                    ),
                ));
            }
            Ok(metrics) => {
                // 1. CPU Threshold Check
                if metrics.cpu_percent >= config.cpu_percent {
                    let severity = if metrics.cpu_percent >= 95.0 {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    };
                    alerts.push(Alert::new(
                        &server.id,
                        &server.name,
                        severity,
                        AlertType::HighCpu,
                        "cpu",
                        format!(
                            "High CPU usage: {:.1}% (threshold: {:.1}%)",
                            metrics.cpu_percent, config.cpu_percent
                        ),
                    ));
                }

                // 2. RAM Threshold Check
                if metrics.memory.percent >= config.ram_percent {
                    let severity = if metrics.memory.percent >= 95.0 {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    };
                    alerts.push(Alert::new(
                        &server.id,
                        &server.name,
                        severity,
                        AlertType::HighRam,
                        "ram",
                        format!(
                            "High RAM usage: {:.1}% ({:.1} GiB / {:.1} GiB, threshold: {:.1}%)",
                            metrics.memory.percent,
                            metrics.memory.used_gib(),
                            metrics.memory.total_gib(),
                            config.ram_percent
                        ),
                    ));
                }

                // 3. Disk Threshold Check
                if metrics.disk.percent >= config.disk_percent {
                    let severity = if metrics.disk.percent >= 95.0 {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    };
                    alerts.push(Alert::new(
                        &server.id,
                        &server.name,
                        severity,
                        AlertType::HighDisk,
                        "disk",
                        format!(
                            "High Disk usage on /: {:.1}% ({:.1} GiB / {:.1} GiB, threshold: {:.1}%)",
                            metrics.disk.percent,
                            metrics.disk.used_gib(),
                            metrics.disk.total_gib(),
                            config.disk_percent
                        ),
                    ));
                }

                // 4. Docker Container Checks
                if metrics.docker.installed {
                    // Check restarting containers (Crash loop)
                    if config.notify_restarting_containers && metrics.docker.restarting > 0 {
                        let restarting_names: Vec<String> = metrics
                            .docker
                            .containers
                            .iter()
                            .filter(|c| c.state == "restarting")
                            .map(|c| c.name.clone())
                            .collect();

                        alerts.push(Alert::new(
                            &server.id,
                            &server.name,
                            AlertSeverity::Critical,
                            AlertType::ContainerRestarting,
                            "docker:restarting",
                            format!(
                                "{} container(s) restarting (crash loop): {}",
                                metrics.docker.restarting,
                                restarting_names.join(", ")
                            ),
                        ));
                    }

                    // Check stopped/exited containers
                    if config.notify_stopped_containers && metrics.docker.stopped > 0 {
                        let stopped_names: Vec<String> = metrics
                            .docker
                            .containers
                            .iter()
                            .filter(|c| c.state == "stopped")
                            .map(|c| c.name.clone())
                            .collect();

                        alerts.push(Alert::new(
                            &server.id,
                            &server.name,
                            AlertSeverity::Warning,
                            AlertType::ContainerStopped,
                            "docker:stopped",
                            format!(
                                "{} container(s) stopped/exited: {}",
                                metrics.docker.stopped,
                                stopped_names.join(", ")
                            ),
                        ));
                    }
                }
            }
        }

        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{ContainerInfo, DiskMetrics, DockerMetrics, MemoryMetrics};

    #[test]
    fn test_alert_evaluation_triggers() {
        let server = ServerConfig {
            id: "node-1".to_string(),
            name: "Node 1".to_string(),
            host: "10.0.0.1".to_string(),
            port: 22,
            user: "root".to_string(),
            key_path: None,
            tags: vec![],
        };

        let config = AlertsConfig {
            webhook_url: None,
            cpu_percent: 80.0,
            ram_percent: 85.0,
            disk_percent: 90.0,
            notify_stopped_containers: true,
            notify_restarting_containers: true,
            cooldown_minutes: 60,
        };

        let metrics = ServerMetrics {
            hostname: "node-1".to_string(),
            uptime_seconds: Some(1000.0),
            cpu_percent: 96.0, // Exceeds 80% (and >= 95% is Critical)
            memory: MemoryMetrics {
                total_bytes: 100,
                used_bytes: 88,
                available_bytes: 12,
                percent: 88.0, // Exceeds 85%
            },
            disk: DiskMetrics {
                total_bytes: 100,
                used_bytes: 92,
                available_bytes: 8,
                percent: 92.0, // Exceeds 90%
            },
            gpus: vec![],
            docker: DockerMetrics {
                installed: true,
                running: 2,
                restarting: 1,
                stopped: 1,
                total: 4,
                containers: vec![
                    ContainerInfo {
                        id: "c1".to_string(),
                        name: "crash-app".to_string(),
                        state: "restarting".to_string(),
                        status: "Restarting (1) 2s ago".to_string(),
                        image: "app:latest".to_string(),
                    },
                    ContainerInfo {
                        id: "c2".to_string(),
                        name: "old-worker".to_string(),
                        state: "stopped".to_string(),
                        status: "Exited (0)".to_string(),
                        image: "worker:latest".to_string(),
                    },
                ],
                error: None,
            },
        };

        let alerts = AlertChecker::evaluate(&server, Ok(&metrics), &config);
        assert_eq!(alerts.len(), 5);

        let cpu_alert = alerts
            .iter()
            .find(|a| a.alert_type == AlertType::HighCpu)
            .unwrap();
        assert_eq!(cpu_alert.severity, AlertSeverity::Critical);

        let ram_alert = alerts
            .iter()
            .find(|a| a.alert_type == AlertType::HighRam)
            .unwrap();
        assert_eq!(ram_alert.severity, AlertSeverity::Warning);

        let disk_alert = alerts
            .iter()
            .find(|a| a.alert_type == AlertType::HighDisk)
            .unwrap();
        assert_eq!(disk_alert.severity, AlertSeverity::Warning);

        let restart_alert = alerts
            .iter()
            .find(|a| a.alert_type == AlertType::ContainerRestarting)
            .unwrap();
        assert_eq!(restart_alert.severity, AlertSeverity::Critical);
        assert!(restart_alert.message.contains("crash-app"));

        let stopped_alert = alerts
            .iter()
            .find(|a| a.alert_type == AlertType::ContainerStopped)
            .unwrap();
        assert_eq!(stopped_alert.severity, AlertSeverity::Warning);
        assert!(stopped_alert.message.contains("old-worker"));
    }

    #[test]
    fn test_alert_evaluation_unreachable() {
        let server = ServerConfig {
            id: "offline-node".to_string(),
            name: "Offline Node".to_string(),
            host: "10.0.0.99".to_string(),
            port: 22,
            user: "root".to_string(),
            key_path: None,
            tags: vec![],
        };

        let config = AlertsConfig::default();
        let alerts = AlertChecker::evaluate(&server, Err("Connection timed out"), &config);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::HostUnreachable);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }
}
