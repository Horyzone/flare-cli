use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use flare_cli::alerts::{Alert, AlertCache, AlertChecker, AlertSeverity, AlertType};
use flare_cli::cli::args::{Cli, Commands, ServerCommands};
use flare_cli::collector::{
    Collector, ContainerInfo, DiskMetrics, DockerMetrics, MemoryMetrics, ServerMetrics,
};
use flare_cli::config::{AlertsConfig, ServerConfig};

#[test]
fn test_cli_parsing_server_add() {
    let args = vec![
        "flare",
        "server",
        "add",
        "-i",
        "web-prod-1",
        "-n",
        "Web Production 1",
        "-H",
        "192.168.1.50",
        "-p",
        "2222",
        "-u",
        "deploy",
        "-k",
        "~/.ssh/id_deploy",
        "-t",
        "prod,web,dokploy",
        "--skip-test",
    ];

    let cli = Cli::try_parse_from(args).expect("Failed to parse server add args");
    match cli.command {
        Commands::Server(ServerCommands::Add(add_args)) => {
            assert_eq!(add_args.id.as_deref(), Some("web-prod-1"));
            assert_eq!(add_args.name.as_deref(), Some("Web Production 1"));
            assert_eq!(add_args.host.as_deref(), Some("192.168.1.50"));
            assert_eq!(add_args.port, Some(2222));
            assert_eq!(add_args.user.as_deref(), Some("deploy"));
            assert_eq!(add_args.key_path.as_deref(), Some("~/.ssh/id_deploy"));
            assert_eq!(
                add_args.tags,
                Some(vec![
                    "prod".to_string(),
                    "web".to_string(),
                    "dokploy".to_string()
                ])
            );
            assert!(add_args.skip_test);
        }
        _ => panic!("Expected Server Add command"),
    }
}

#[test]
fn test_cli_parsing_server_list_and_remove() {
    let list_args = vec!["flare", "server", "list", "--tag", "prod"];
    let cli = Cli::try_parse_from(list_args).unwrap();
    match cli.command {
        Commands::Server(ServerCommands::List(args)) => {
            assert_eq!(args.tag.as_deref(), Some("prod"));
        }
        _ => panic!("Expected Server List command"),
    }

    let remove_args = vec!["flare", "server", "remove", "web-prod-1", "--force"];
    let cli = Cli::try_parse_from(remove_args).unwrap();
    match cli.command {
        Commands::Server(ServerCommands::Remove(args)) => {
            assert_eq!(args.id.as_deref(), Some("web-prod-1"));
            assert!(args.force);
        }
        _ => panic!("Expected Server Remove command"),
    }
}

#[test]
fn test_cli_parsing_status_and_check() {
    let status_args = vec!["flare", "status", "node-1", "--json", "--timeout", "15"];
    let cli = Cli::try_parse_from(status_args).unwrap();
    match cli.command {
        Commands::Status(args) => {
            assert_eq!(args.server_id.as_deref(), Some("node-1"));
            assert!(args.json);
            assert_eq!(args.timeout, 15);
        }
        _ => panic!("Expected Status command"),
    }

    let check_args = vec![
        "flare",
        "check",
        "--dry-run",
        "--force",
        "--tag",
        "dokploy",
        "--timeout",
        "20",
    ];
    let cli = Cli::try_parse_from(check_args).unwrap();
    match cli.command {
        Commands::Check(args) => {
            assert!(args.dry_run);
            assert!(args.force);
            assert_eq!(args.tag.as_deref(), Some("dokploy"));
            assert_eq!(args.timeout, 20);
        }
        _ => panic!("Expected Check command"),
    }
}

#[test]
fn test_collector_probe_posix_fallback_payload() {
    // Exact JSON format emitted by the POSIX shell fallback probe script
    let shell_fallback_raw = r#"
__FLARE_JSON_START__{"hostname":"alpine-vm","uptime_seconds":86400,"cpu_percent":5.2,"memory":{"total_bytes":8589934592,"used_bytes":2147483648,"available_bytes":6442450944,"percent":25.0},"disk":{"total_bytes":53687091200,"used_bytes":10737418240,"available_bytes":42949672960,"percent":20.0},"gpus":[],"docker":{"installed":false,"running":0,"restarting":0,"stopped":0,"total":0,"containers":[]}}__FLARE_JSON_END__
"#;

    let metrics = Collector::parse_output(shell_fallback_raw, "alpine-vm")
        .expect("Failed to parse shell fallback payload");
    assert_eq!(metrics.hostname, "alpine-vm");
    assert_eq!(metrics.uptime_seconds, Some(86400.0));
    assert_eq!(metrics.cpu_percent, 5.2);
    assert_eq!(metrics.memory.percent, 25.0);
    assert_eq!(metrics.memory.total_gib(), 8.0);
    assert_eq!(metrics.disk.percent, 20.0);
    assert_eq!(metrics.disk.total_gib(), 50.0);
    assert!(!metrics.docker.installed);
    assert!(metrics.gpus.is_empty());
}

#[test]
fn test_collector_probe_fallback_braces_without_delimiters() {
    // In case delimiters are stripped or missing, brace extraction should succeed
    let raw = r#"
Warning: Permanently added '192.168.1.1' (ED25519) to the list of known hosts.
{
  "hostname": "bare-node",
  "uptime_seconds": 120.0,
  "cpu_percent": 1.0,
  "memory": { "total_bytes": 1000, "used_bytes": 200, "available_bytes": 800, "percent": 20.0 },
  "disk": { "total_bytes": 5000, "used_bytes": 1000, "available_bytes": 4000, "percent": 20.0 },
  "gpus": [],
  "docker": { "installed": false, "running": 0, "restarting": 0, "stopped": 0, "total": 0, "containers": [] }
}
"#;

    let metrics =
        Collector::parse_output(raw, "bare-node").expect("Should parse via outer braces fallback");
    assert_eq!(metrics.hostname, "bare-node");
    assert_eq!(metrics.cpu_percent, 1.0);
}

#[test]
fn test_collector_probe_invalid_json() {
    let bad_output = "Connection refused\nPermission denied";
    let res = Collector::parse_output(bad_output, "failing-node");
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("failing-node"));
}

#[test]
fn test_uptime_formatting_variations() {
    let mut m = ServerMetrics {
        hostname: "test".to_string(),
        uptime_seconds: Some(45.0),
        cpu_percent: 0.0,
        memory: MemoryMetrics::default(),
        disk: DiskMetrics::default(),
        gpus: vec![],
        docker: DockerMetrics::default(),
    };
    assert_eq!(m.format_uptime(), "0m");

    m.uptime_seconds = Some(150.0); // 2m 30s
    assert_eq!(m.format_uptime(), "2m");

    m.uptime_seconds = Some(3661.0); // 1h 1m 1s
    assert_eq!(m.format_uptime(), "1h 1m");

    m.uptime_seconds = Some(90060.0); // 1d 1h 1m
    assert_eq!(m.format_uptime(), "1d 1h 1m");

    m.uptime_seconds = None;
    assert_eq!(m.format_uptime(), "unknown");
}

#[test]
fn test_alert_cache_pruning_and_serialization() {
    let mut cache = AlertCache::default();
    let alert_old = Alert::new(
        "s1",
        "S1",
        AlertSeverity::Warning,
        AlertType::HighCpu,
        "cpu",
        "Old CPU",
    );
    let alert_fresh = Alert::new(
        "s2",
        "S2",
        AlertSeverity::Critical,
        AlertType::HighDisk,
        "disk",
        "Fresh Disk",
    );

    cache.record_notification(&alert_old);
    cache.record_notification(&alert_fresh);

    // Age alert_old by 35 days
    if let Some(entry) = cache.entries.get_mut(&alert_old.cache_key()) {
        entry.last_notified = Utc::now() - ChronoDuration::days(35);
    }

    // Prune entries older than 30 days
    cache.prune(30);

    assert!(!cache.entries.contains_key(&alert_old.cache_key()));
    assert!(cache.entries.contains_key(&alert_fresh.cache_key()));

    // Test JSON serialization roundtrip
    let serialized = serde_json::to_string(&cache).unwrap();
    let deserialized: AlertCache = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.entries.len(), 1);
    assert!(deserialized.entries.contains_key(&alert_fresh.cache_key()));
}

#[test]
fn test_alert_evaluation_edge_cases() {
    let server = ServerConfig {
        id: "edge-srv".to_string(),
        name: "Edge Server".to_string(),
        host: "10.10.10.10".to_string(),
        port: 22,
        user: "root".to_string(),
        key_path: None,
        tags: vec![],
    };

    let config = AlertsConfig {
        webhook_url: None,
        cpu_percent: 80.0,
        ram_percent: 80.0,
        disk_percent: 80.0,
        notify_stopped_containers: false, // Disabled stopped check
        notify_restarting_containers: true,
        cooldown_minutes: 60,
    };

    // Exactly at threshold -> should trigger
    let metrics = ServerMetrics {
        hostname: "edge-srv".to_string(),
        uptime_seconds: Some(100.0),
        cpu_percent: 80.0,
        memory: MemoryMetrics {
            total_bytes: 100,
            used_bytes: 80,
            available_bytes: 20,
            percent: 80.0,
        },
        disk: DiskMetrics {
            total_bytes: 100,
            used_bytes: 80,
            available_bytes: 20,
            percent: 80.0,
        },
        gpus: vec![],
        docker: DockerMetrics {
            installed: true,
            running: 5,
            restarting: 0,
            stopped: 3, // Stopped containers exist, but notify_stopped_containers is false
            total: 8,
            containers: vec![ContainerInfo {
                id: "c1".to_string(),
                name: "stopped-1".to_string(),
                state: "stopped".to_string(),
                status: "Exited (0)".to_string(),
                image: "img:1".to_string(),
            }],
            error: None,
        },
    };

    let alerts = AlertChecker::evaluate(&server, Ok(&metrics), &config);
    // Should trigger CPU, RAM, Disk, but NOT stopped containers
    assert_eq!(alerts.len(), 3);
    assert!(alerts.iter().any(|a| a.alert_type == AlertType::HighCpu));
    assert!(alerts.iter().any(|a| a.alert_type == AlertType::HighRam));
    assert!(alerts.iter().any(|a| a.alert_type == AlertType::HighDisk));
    assert!(!alerts
        .iter()
        .any(|a| a.alert_type == AlertType::ContainerStopped));
}
