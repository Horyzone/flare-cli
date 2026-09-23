use anyhow::{Context, Result};
use std::time::Instant;

use super::models::ServerMetrics;
use crate::config::ServerConfig;
use crate::ssh::SshClient;

/// Complete composite probe script that runs on the remote host without any external dependencies.
/// Prefers Python 3 (standard library only) and falls back to a POSIX shell + awk script.
pub const REMOTE_PROBE_SCRIPT: &str = r#"sh -c '
if command -v python3 >/dev/null 2>&1; then
python3 -c "
import json, os, socket, subprocess, sys, time

def get_uptime():
    try:
        with open(\"/proc/uptime\", \"r\") as f:
            return float(f.readline().split()[0])
    except Exception:
        return None

def get_cpu():
    try:
        def read_stat():
            with open(\"/proc/stat\", \"r\") as f:
                fields = [float(x) for x in f.readline().split()[1:]]
            idle = fields[3] + (fields[4] if len(fields) > 4 else 0)
            total = sum(fields)
            return idle, total
        idle1, total1 = read_stat()
        time.sleep(0.3)
        idle2, total2 = read_stat()
        diff_idle = idle2 - idle1
        diff_total = total2 - total1
        if diff_total > 0:
            return round(100.0 * (1.0 - diff_idle / diff_total), 1)
    except Exception:
        pass
    try:
        load1, _, _ = os.getloadavg()
        cpus = os.cpu_count() or 1
        return round((load1 / cpus) * 100.0, 1)
    except Exception:
        return 0.0

def get_memory():
    try:
        meminfo = {}
        with open(\"/proc/meminfo\", \"r\") as f:
            for line in f:
                parts = line.split(\":\")
                if len(parts) == 2:
                    k = parts[0].strip()
                    v = parts[1].strip().split()[0]
                    meminfo[k] = int(v) * 1024
        total = meminfo.get(\"MemTotal\", 0)
        avail = meminfo.get(\"MemAvailable\", meminfo.get(\"MemFree\", 0) + meminfo.get(\"Buffers\", 0) + meminfo.get(\"Cached\", 0))
        used = total - avail
        pct = round((used / total * 100.0), 1) if total > 0 else 0.0
        return {\"total_bytes\": total, \"used_bytes\": used, \"available_bytes\": avail, \"percent\": pct}
    except Exception:
        return {\"total_bytes\": 0, \"used_bytes\": 0, \"available_bytes\": 0, \"percent\": 0.0}

def get_disk():
    try:
        st = os.statvfs(\"/\")
        total = st.f_blocks * st.f_frsize
        avail = st.f_bavail * st.f_frsize
        used = total - avail
        pct = round((used / total * 100.0), 1) if total > 0 else 0.0
        return {\"total_bytes\": total, \"used_bytes\": used, \"available_bytes\": avail, \"percent\": pct}
    except Exception:
        return {\"total_bytes\": 0, \"used_bytes\": 0, \"available_bytes\": 0, \"percent\": 0.0}

def get_gpu():
    try:
        out = subprocess.check_output(
            [\"nvidia-smi\", \"--query-gpu=name,utilization.gpu,temperature.gpu,memory.total,memory.used\", \"--format=csv,noheader,nounits\"],
            stderr=subprocess.DEVNULL, timeout=2
        ).decode().strip()
        gpus = []
        for line in out.splitlines():
            parts = [p.strip() for p in line.split(\",\")]
            if len(parts) >= 5:
                gpus.append({
                    \"name\": parts[0],
                    \"utilization_percent\": float(parts[1]),
                    \"temperature_c\": float(parts[2]),
                    \"memory_total_mb\": float(parts[3]),
                    \"memory_used_mb\": float(parts[4])
                })
        return gpus
    except Exception:
        return []

def get_docker():
    try:
        out = subprocess.check_output(
            [\"docker\", \"ps\", \"-a\", \"--format\", \"{{.ID}}\\t{{.Names}}\\t{{.State}}\\t{{.Status}}\\t{{.Image}}\"],
            stderr=subprocess.DEVNULL, timeout=4
        ).decode().strip()
        containers = []
        running = 0
        restarting = 0
        stopped = 0
        if out:
            for line in out.splitlines():
                parts = line.split(\"\\t\")
                if len(parts) >= 5:
                    cid, name, state, status, image = parts[0], parts[1], parts[2].lower(), parts[3], parts[4]
                    if \"restart\" in state or \"restarting\" in status.lower():
                        restarting += 1
                        norm_state = \"restarting\"
                    elif \"running\" in state or state == \"up\":
                        running += 1
                        norm_state = \"running\"
                    else:
                        stopped += 1
                        norm_state = \"stopped\"
                    containers.append({
                        \"id\": cid,
                        \"name\": name,
                        \"state\": norm_state,
                        \"status\": status,
                        \"image\": image
                    })
        return {
            \"installed\": True,
            \"running\": running,
            \"restarting\": restarting,
            \"stopped\": stopped,
            \"total\": len(containers),
            \"containers\": containers
        }
    except Exception as e:
        return {\"installed\": False, \"running\": 0, \"restarting\": 0, \"stopped\": 0, \"total\": 0, \"containers\": [], \"error\": str(e)}

data = {
    \"hostname\": socket.gethostname(),
    \"uptime_seconds\": get_uptime(),
    \"cpu_percent\": get_cpu(),
    \"memory\": get_memory(),
    \"disk\": get_disk(),
    \"gpus\": get_gpu(),
    \"docker\": get_docker()
}
print(\"__FLARE_JSON_START__\" + json.dumps(data) + \"__FLARE_JSON_END__\")
"
else
# POSIX shell fallback when python3 is not installed
HOST=$(hostname 2>/dev/null || uname -n)
UPTIME=$(awk '{print $1}' /proc/uptime 2>/dev/null || echo 0)

# Memory from /proc/meminfo
TOTAL_MEM=$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
AVAIL_MEM=$(awk '/MemAvailable/ {print $2}' /proc/meminfo 2>/dev/null)
if [ -z "$AVAIL_MEM" ]; then
    FREE_MEM=$(awk '/MemFree/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
    BUFF_MEM=$(awk '/Buffers/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
    CACH_MEM=$(awk '/^Cached/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
    AVAIL_MEM=$((FREE_MEM + BUFF_MEM + CACH_MEM))
fi
TOTAL_MEM_BYTES=$((TOTAL_MEM * 1024))
AVAIL_MEM_BYTES=$((AVAIL_MEM * 1024))
USED_MEM_BYTES=$((TOTAL_MEM_BYTES - AVAIL_MEM_BYTES))
if [ "$TOTAL_MEM_BYTES" -gt 0 ]; then
    MEM_PCT=$(awk -v u="$USED_MEM_BYTES" -v t="$TOTAL_MEM_BYTES" 'BEGIN {printf "%.1f", (u/t)*100}')
else
    MEM_PCT=0.0
fi

# Disk from df /
DISK_INFO=$(df -k / | tail -n 1)
DISK_TOTAL_K=$(echo "$DISK_INFO" | awk '{print $2}')
DISK_USED_K=$(echo "$DISK_INFO" | awk '{print $3}')
DISK_AVAIL_K=$(echo "$DISK_INFO" | awk '{print $4}')
DISK_TOTAL_BYTES=$((DISK_TOTAL_K * 1024))
DISK_USED_BYTES=$((DISK_USED_K * 1024))
DISK_AVAIL_BYTES=$((DISK_AVAIL_K * 1024))
if [ "$DISK_TOTAL_BYTES" -gt 0 ]; then
    DISK_PCT=$(awk -v u="$DISK_USED_BYTES" -v t="$DISK_TOTAL_BYTES" 'BEGIN {printf "%.1f", (u/t)*100}')
else
    DISK_PCT=0.0
fi

# CPU approximation from loadavg
LOAD1=$(awk '{print $1}' /proc/loadavg 2>/dev/null || echo 0)
CPUS=$(grep -c ^processor /proc/cpuinfo 2>/dev/null || echo 1)
CPU_PCT=$(awk -v l="$LOAD1" -v c="$CPUS" 'BEGIN {pct=(l/c)*100; if(pct>100)pct=100; printf "%.1f", pct}')

echo "__FLARE_JSON_START__{\"hostname\":\"$HOST\",\"uptime_seconds\":$UPTIME,\"cpu_percent\":$CPU_PCT,\"memory\":{\"total_bytes\":$TOTAL_MEM_BYTES,\"used_bytes\":$USED_MEM_BYTES,\"available_bytes\":$AVAIL_MEM_BYTES,\"percent\":$MEM_PCT},\"disk\":{\"total_bytes\":$DISK_TOTAL_BYTES,\"used_bytes\":$DISK_USED_BYTES,\"available_bytes\":$DISK_AVAIL_BYTES,\"percent\":$DISK_PCT},\"gpus\":[],\"docker\":{\"installed\":false,\"running\":0,\"restarting\":0,\"stopped\":0,\"total\":0,\"containers\":[]}}__FLARE_JSON_END__"
fi
'
"#;

pub struct Collector;

impl Collector {
    /// Executes the remote probe script over SSH and parses the resulting metrics.
    pub async fn collect(
        server: &ServerConfig,
        timeout_secs: u64,
    ) -> Result<(ServerMetrics, u128)> {
        let start = Instant::now();
        let raw_output = SshClient::run_command(server, REMOTE_PROBE_SCRIPT, timeout_secs)
            .await
            .with_context(|| format!("Failed to run collector probe on {}", server.id))?;

        let duration_ms = start.elapsed().as_millis();
        let metrics = Self::parse_output(&raw_output, &server.id)?;

        Ok((metrics, duration_ms))
    }

    /// Extracts the JSON payload delimited by `__FLARE_JSON_START__` and `__FLARE_JSON_END__`.
    /// Falls back to finding the outer `{` and `}` braces if delimiters are absent.
    pub fn parse_output(raw: &str, server_id: &str) -> Result<ServerMetrics> {
        let json_str = if let Some(start) = raw.find("__FLARE_JSON_START__") {
            let after_start = &raw[start + "__FLARE_JSON_START__".len()..];
            if let Some(end) = after_start.find("__FLARE_JSON_END__") {
                &after_start[..end]
            } else {
                after_start
            }
        } else if let (Some(first_brace), Some(last_brace)) = (raw.find('{'), raw.rfind('}')) {
            if first_brace <= last_brace {
                &raw[first_brace..=last_brace]
            } else {
                raw
            }
        } else {
            raw
        };

        let metrics: ServerMetrics = serde_json::from_str(json_str.trim()).with_context(|| {
            format!(
                "Failed to deserialize metrics JSON from server '{}'. Raw output was:\n{}",
                server_id,
                raw.chars().take(400).collect::<String>()
            )
        })?;

        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_delimited_output() {
        let sample = r#"
Welcome to Ubuntu 22.04.4 LTS
Last login: Wed Sep 23 18:00:00 2026 from 1.2.3.4
__FLARE_JSON_START__{
  "hostname": "prod-vps",
  "uptime_seconds": 12345.6,
  "cpu_percent": 18.5,
  "memory": {
    "total_bytes": 17179869184,
    "used_bytes": 8589934592,
    "available_bytes": 8589934592,
    "percent": 50.0
  },
  "disk": {
    "total_bytes": 107374182400,
    "used_bytes": 42949672960,
    "available_bytes": 64424509440,
    "percent": 40.0
  },
  "gpus": [
    {
      "name": "NVIDIA A10G",
      "utilization_percent": 35.0,
      "temperature_c": 58.0,
      "memory_total_mb": 24000.0,
      "memory_used_mb": 6000.0
    }
  ],
  "docker": {
    "installed": true,
    "running": 3,
    "restarting": 1,
    "stopped": 0,
    "total": 4,
    "containers": [
      {
        "id": "c1234567890a",
        "name": "dokploy-traefik",
        "state": "running",
        "status": "Up 2 days",
        "image": "traefik:v3.0"
      },
      {
        "id": "c9876543210b",
        "name": "api-service",
        "state": "restarting",
        "status": "Restarting (1) 5 seconds ago",
        "image": "myorg/api:latest"
      }
    ]
  }
}__FLARE_JSON_END__
logout
Connection closed.
"#;
        let metrics = Collector::parse_output(sample, "test-server").unwrap();
        assert_eq!(metrics.hostname, "prod-vps");
        assert_eq!(metrics.cpu_percent, 18.5);
        assert_eq!(metrics.memory.percent, 50.0);
        assert_eq!(metrics.memory.total_gib(), 16.0);
        assert_eq!(metrics.memory.used_gib(), 8.0);
        assert_eq!(metrics.disk.percent, 40.0);
        assert_eq!(metrics.disk.total_gib(), 100.0);
        assert_eq!(metrics.disk.used_gib(), 40.0);
        assert_eq!(metrics.gpus.len(), 1);
        assert_eq!(metrics.gpus[0].name, "NVIDIA A10G");
        assert_eq!(metrics.docker.running, 3);
        assert_eq!(metrics.docker.restarting, 1);
        assert_eq!(metrics.docker.containers.len(), 2);
    }
}
