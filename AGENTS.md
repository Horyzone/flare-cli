# AGENTS.md - Developer & Agent Guide for `flare-cli`

Welcome to `flare-cli` (`flare`). This document serves as the architectural reference and development guide for human contributors and autonomous AI agents working on this codebase.

---

## 1. Project Overview & Philosophy

- **Crate Name:** `flare-cli`
- **Binary Name:** `flare` (configured explicitly via `[[bin]]` in `Cargo.toml`)
- **Language / Edition:** Rust 2021
- **Core Mission:** Lightweight, agentless CLI operations and real-time monitoring tool designed for remote Linux servers running Docker and Dokploy.
- **Philosophy:**
  - **Zero-Agent Footprint:** No heavy daemons or agents to install or maintain on target machines.
  - **Native SSH Transport:** Relies on the operator's existing `ssh` configuration, `ssh-agent`, custom keys, and `~/.ssh/config`.
  - **Resilient Probing:** Remote metrics are gathered in a single round-trip using an embedded composite probe (Python 3 standard library with POSIX shell fallback).
  - **Sentinel Alerting:** Automated health checks designed for local cron execution, with cooldown caching and webhook dispatching (ntfy.sh, Discord, generic webhooks).

---

## 2. Architecture & Codebase Map

```
flare/
├── Cargo.toml                  # Package definition (binary: flare, dependencies)
├── AGENTS.md                   # Agent & developer architectural documentation
├── src/
│   ├── main.rs                 # CLI entry point, runtime setup, and global error handling
│   │
│   ├── cli/                    # CLI parsing, subcommands, and terminal presentations
│   │   ├── mod.rs              # Subcommand dispatcher (`run`)
│   │   ├── args.rs             # Clap derive structs and argument hierarchies
│   │   ├── server.rs           # `flare server add`, `list`, `remove` implementations
│   │   ├── ssh_cmd.rs          # `flare ssh [server_id]` interactive shell launcher
│   │   ├── status.rs           # `flare status` dashboard (single & parallel multi-server)
│   │   └── check.rs            # `flare check` sentinel inspection & alerting workflow
│   │
│   ├── config/                 # Inventory & configuration management
│   │   ├── mod.rs              # Module re-exports
│   │   ├── models.rs           # ServerConfig, AlertsConfig, AppConfig schemas
│   │   └── manager.rs          # ConfigManager (~/.config/flare/config.yaml load/save/filter)
│   │
│   ├── ssh/                    # SSH protocol & process execution layer
│   │   ├── mod.rs              # Module re-exports
│   │   └── client.rs           # SshClient: TCP check, SSH authentication, run_command, TTY shell
│   │
│   ├── collector/              # Remote telemetry gathering
│   │   ├── mod.rs              # Module re-exports
│   │   ├── models.rs           # ServerMetrics, MemoryMetrics, DiskMetrics, GpuMetrics, DockerMetrics
│   │   └── probe.rs            # Embedded composite remote script & resilient JSON parser
│   │
│   └── alerts/                 # Sentinel evaluation & notification system
│       ├── mod.rs              # Module re-exports
│       ├── models.rs           # Alert, AlertSeverity, AlertType schemas
│       ├── checker.rs          # Threshold comparison & rule engine
│       ├── cache.rs            # Deduplication cooldown cache (~/.cache/flare/alert_cache.json)
│       └── notifier.rs         # Webhook dispatch engine (ntfy.sh, Discord, generic JSON)
```

---

## 3. Subsystem Breakdown & Design Principles

### A. Configuration (`src/config/`)
- Default path: `~/.config/flare/config.yaml`.
- Supports tilde expansion (`~/...`) for SSH private key paths.
- Automatically initializes a documented template config if the file does not exist.
- Validates uniqueness of server IDs (`slug`).

### B. SSH Transport (`src/ssh/`)
- Direct invocation of the local `ssh` binary rather than bundling a pure-Rust SSH client:
  - Preserves user's OpenSSH features (certificates, FIDO tokens, `ssh-agent`, ProxyJump).
  - Uses non-interactive security flags (`BatchMode=yes`, `StrictHostKeyChecking=accept-new`, `ConnectTimeout=6`).
- Fast dual-phase reachability check:
  1. Instant asynchronous TCP handshake test via `tokio::net::TcpStream`.
  2. SSH auth probe (`ssh ... true`).
- Interactive shells: passes control to `ssh -t` with standard streams inherited (`stdin`, `stdout`, `stderr`).

### C. Remote Telemetry Probe (`src/collector/`)
- One-liner composite script:
  - Detects `python3`. If present, uses standard library modules (`os`, `json`, `subprocess`, `time`, `socket`) to sample CPU delta (`/proc/stat`), RAM (`/proc/meminfo`), root disk (`statvfs`), NVIDIA GPU (`nvidia-smi`), and Docker (`docker ps -a`).
  - Fallback: POSIX `/bin/sh` + `awk` + `df` ensures zero-dependency execution on stripped containers or Alpine nodes.
- **Robust JSON Extraction:** The output is framed with markers `__FLARE_JSON_START__` and `__FLARE_JSON_END__`. The parser extracts only the contained payload, preventing SSH banners, MOTDs, or command noise from breaking deserialization.

### D. Sentinel Alerting & Caching (`src/alerts/`)
- Health evaluation against configurable thresholds:
  - Host unreachable (Critical)
  - CPU usage > threshold (Warning / Critical if $\ge 95\%$)
  - RAM usage > threshold (Warning / Critical if $\ge 95\%$)
  - Disk usage > threshold (Warning / Critical if $\ge 95\%$)
  - Docker containers restarting / crash loop (Critical)
  - Docker containers stopped / exited (Warning)
- **Cooldown Deduplication Cache:**
  - Stored in `~/.cache/flare/alert_cache.json`.
  - Keys based on `{server_id}:{alert_type}:{target}`.
  - Prevents alert notification storms by enforcing a configurable cooldown window (`cooldown_minutes`, default 60 min).
  - Can be bypassed via `--force` or simulated via `--dry-run`.
- **Multi-Destination Notifier:**
  - `ntfy.sh`: Markdown payload, custom priority headers (`X-Priority: high/urgent`), and emoji tags.
  - Discord webhooks: Rich color-coded embeds.
  - Generic webhooks: Clean JSON payload.

---

## 4. Development Workflow & Rules for Agents

### Running Tests
Always ensure all unit tests pass before committing:
```bash
cargo test
```

### Checking Compilation and Warnings
Zero warnings must be maintained across all modules:
```bash
cargo check
```

### Building the Binary
```bash
cargo build --release
# Binary available at target/release/flare
```

### Git Commit Guidelines
Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat: ...` for new features or subcommands.
- `fix: ...` for bug fixes.
- `docs: ...` for documentation updates.
- `test: ...` for adding or improving test coverage.
- `refactor: ...` for code quality improvements without behavioral change.

Make atomic commits at logical milestones of implementation.

> **CRITICAL RULE FOR AGENTS:**
> **NEVER run `git push`.** Always perform **local commits only** (`git commit`). The operator manages all remote pushes and branch synchronizations manually.

---

## 5. Planned Roadmap & Extensibility Points

When extending `flare-cli`, follow the established module boundaries:
1. **Remote Log Streaming:** Add `flare logs <server_id> [container_name]` via `tokio::process::Command` streaming `docker logs -f --tail`.
2. **Snapshot / Backup Command:** Add `flare snapshot <server_id>` to invoke remote Dokploy backup procedures.
3. **Custom Remote Script Execution:** Add `flare exec <server_id|--tag> -- <command>` to run arbitrary remote commands in parallel across fleets.
