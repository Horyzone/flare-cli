use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use std::time::Duration;

use super::models::{Alert, AlertSeverity};

pub struct Notifier;

impl Notifier {
    /// Dispatches a batch of alerts to the configured webhook URL.
    pub async fn dispatch(webhook_url: &str, alerts: &[Alert]) -> Result<()> {
        if alerts.is_empty() {
            return Ok(());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client for alerting")?;

        if webhook_url.contains("ntfy.sh") || webhook_url.contains("/ntfy") {
            Self::dispatch_ntfy(&client, webhook_url, alerts).await
        } else if webhook_url.contains("discord.com/api/webhooks") {
            Self::dispatch_discord(&client, webhook_url, alerts).await
        } else {
            Self::dispatch_generic_json(&client, webhook_url, alerts).await
        }
    }

    /// Sends formatted alert to an ntfy.sh topic.
    async fn dispatch_ntfy(client: &reqwest::Client, url: &str, alerts: &[Alert]) -> Result<()> {
        let has_critical = alerts.iter().any(|a| a.severity == AlertSeverity::Critical);
        let priority = if has_critical { "urgent" } else { "high" };
        let tags = if has_critical {
            "rotating_light,warning,server"
        } else {
            "warning,server"
        };

        let title = if alerts.len() == 1 {
            format!(
                "🚨 Flare Alert: {} ({})",
                alerts[0].server_name,
                alerts[0].severity.as_str()
            )
        } else {
            format!("🚨 Flare: {} alerts triggered", alerts.len())
        };

        let mut body = String::new();
        for alert in alerts {
            let emoji = match alert.severity {
                AlertSeverity::Critical => "🔴",
                AlertSeverity::Warning => "🟡",
            };
            body.push_str(&format!(
                "{} **[{}]** `{}`: {}\n\n",
                emoji, alert.server_name, alert.target, alert.message
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert("Title", HeaderValue::from_str(&title)?);
        headers.insert("Priority", HeaderValue::from_static(priority));
        headers.insert("Tags", HeaderValue::from_static(tags));
        headers.insert("Markdown", HeaderValue::from_static("yes"));

        let resp = client
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .with_context(|| format!("Failed to send alert to ntfy endpoint {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("ntfy alert failed with HTTP status {}: {}", status, text);
        }

        Ok(())
    }

    /// Sends structured alert payload to a Discord webhook.
    async fn dispatch_discord(client: &reqwest::Client, url: &str, alerts: &[Alert]) -> Result<()> {
        let mut embeds = Vec::new();
        for alert in alerts {
            let color = match alert.severity {
                AlertSeverity::Critical => 0xE74C3C, // Red
                AlertSeverity::Warning => 0xF39C12,  // Orange/Yellow
            };

            embeds.push(serde_json::json!({
                "title": format!("{} Alert - {}", alert.severity.as_str(), alert.server_name),
                "description": alert.message,
                "color": color,
                "fields": [
                    { "name": "Target", "value": alert.target, "inline": true },
                    { "name": "Type", "value": alert.alert_type.as_str(), "inline": true }
                ],
                "timestamp": alert.timestamp.to_rfc3339()
            }));
        }

        let payload = serde_json::json!({
            "username": "Flare Sentinel",
            "avatar_url": "https://raw.githubusercontent.com/favicon.ico",
            "embeds": embeds
        });

        let resp = client
            .post(url)
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("Failed to send alert to Discord webhook {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Discord alert failed with HTTP status {}: {}", status, text);
        }

        Ok(())
    }

    /// Sends generic JSON payload to any standard webhook.
    async fn dispatch_generic_json(
        client: &reqwest::Client,
        url: &str,
        alerts: &[Alert],
    ) -> Result<()> {
        let payload = serde_json::json!({
            "source": "flare-cli",
            "total_alerts": alerts.len(),
            "alerts": alerts
        });

        let resp = client
            .post(url)
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("Failed to send alert to generic webhook {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Generic webhook failed with HTTP status {}: {}",
                status,
                text
            );
        }

        Ok(())
    }
}
