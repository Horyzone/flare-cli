use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::models::Alert;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub first_seen: DateTime<Utc>,
    pub last_notified: DateTime<Utc>,
    pub notification_count: u32,
    pub last_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlertCache {
    pub entries: HashMap<String, CacheEntry>,
}

impl AlertCache {
    /// Returns the cache file path: `~/.cache/flare/alert_cache.json`
    pub fn cache_file_path() -> Result<PathBuf> {
        let base_dir = dirs::cache_dir()
            .or_else(dirs::config_dir)
            .context("Could not determine user cache directory")?;
        Ok(base_dir.join("flare").join("alert_cache.json"))
    }

    /// Loads the cache from disk, or returns an empty cache if it doesn't exist.
    pub fn load() -> Self {
        match Self::cache_file_path() {
            Ok(path) if path.exists() => fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str::<Self>(&data).ok())
                .unwrap_or_default(),
            _ => Self::default(),
        }
    }

    /// Saves the cache to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create alert cache dir {}", parent.display())
            })?;
        }

        let json = serde_json::to_string_pretty(self).context("Failed to serialize alert cache")?;

        fs::write(&path, json)
            .with_context(|| format!("Failed to write alert cache to {}", path.display()))?;

        Ok(())
    }

    /// Determines if an alert should be notified based on the configured cooldown window.
    pub fn should_notify(&self, alert: &Alert, cooldown_minutes: u64) -> bool {
        let key = alert.cache_key();
        match self.entries.get(&key) {
            None => true,
            Some(entry) => {
                let cooldown = Duration::minutes(cooldown_minutes as i64);
                let elapsed = Utc::now().signed_duration_since(entry.last_notified);
                elapsed >= cooldown
            }
        }
    }

    /// Records that an alert was notified, updating the cache entry.
    pub fn record_notification(&mut self, alert: &Alert) {
        let key = alert.cache_key();
        let now = Utc::now();

        self.entries
            .entry(key)
            .and_modify(|entry| {
                entry.last_notified = now;
                entry.notification_count += 1;
                entry.last_message = alert.message.clone();
            })
            .or_insert(CacheEntry {
                first_seen: now,
                last_notified: now,
                notification_count: 1,
                last_message: alert.message.clone(),
            });
    }

    /// Cleans up cache entries that haven't been seen for more than `max_age_days`.
    #[allow(dead_code)]
    pub fn prune(&mut self, max_age_days: i64) {
        let threshold = Duration::days(max_age_days);
        let now = Utc::now();
        self.entries
            .retain(|_, entry| now.signed_duration_since(entry.last_notified) < threshold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::models::{AlertSeverity, AlertType};

    #[test]
    fn test_alert_cache_cooldown() {
        let mut cache = AlertCache::default();
        let alert = Alert::new(
            "srv-1",
            "Server 1",
            AlertSeverity::Warning,
            AlertType::HighCpu,
            "cpu",
            "High CPU: 92%",
        );

        // First notification should be allowed
        assert!(cache.should_notify(&alert, 60));

        // Record notification
        cache.record_notification(&alert);

        // Immediate subsequent notification must be blocked by cooldown
        assert!(!cache.should_notify(&alert, 60));

        // Different alert target should be allowed
        let alert_disk = Alert::new(
            "srv-1",
            "Server 1",
            AlertSeverity::Warning,
            AlertType::HighDisk,
            "disk",
            "High Disk: 89%",
        );
        assert!(cache.should_notify(&alert_disk, 60));

        // If last notification was 61 minutes ago, it should be allowed
        if let Some(entry) = cache.entries.get_mut(&alert.cache_key()) {
            entry.last_notified = Utc::now() - Duration::minutes(61);
        }
        assert!(cache.should_notify(&alert, 60));
    }
}
