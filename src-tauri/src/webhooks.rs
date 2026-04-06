use serde::{Serialize, Deserialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tauri::Emitter;

static ACTIVE_WEBHOOKS: AtomicUsize = AtomicUsize::new(0);
const MAX_CONCURRENT_WEBHOOKS: usize = 5;
const MAX_DELIVERY_LOG: usize = 20;

struct WebhookGuard;
impl Drop for WebhookGuard {
    fn drop(&mut self) {
        ACTIVE_WEBHOOKS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// In-memory ring buffer of the last 20 webhook delivery attempts.
static DELIVERY_LOG: Mutex<Vec<WebhookDelivery>> = Mutex::new(Vec::new());

/// A single webhook delivery attempt record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDelivery {
    pub id: u64,
    pub timestamp: String,
    pub url: String,
    pub event_type: String,
    pub status_code: Option<u16>,
    pub success: bool,
    pub response_snippet: String,
}

fn record_delivery(url: String, event_type: String, status_code: Option<u16>, success: bool, response_snippet: String) {
    let timestamp = chrono::Utc::now().to_rfc3339();
    // Use nanoseconds as a simple monotonic ID
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let delivery = WebhookDelivery { id, timestamp, url, event_type, status_code, success, response_snippet };

    if let Ok(mut log) = DELIVERY_LOG.lock() {
        log.push(delivery);
        // Keep only the last MAX_DELIVERY_LOG entries
        if log.len() > MAX_DELIVERY_LOG {
            let drain_count = log.len() - MAX_DELIVERY_LOG;
            log.drain(0..drain_count);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    #[serde(default = "default_webhook_type")]
    pub webhook_type: String,
    #[serde(default = "default_true")]
    pub on_task_complete: bool,
    #[serde(default)]
    pub on_task_fail: bool,
    #[serde(default)]
    pub on_pipeline_complete: bool,
    /// Optional allowlist of event types to deliver. Empty = all events allowed.
    #[serde(default)]
    pub event_filter: Vec<String>,
    /// ISO-8601 timestamp of the most recent delivery attempt.
    #[serde(default)]
    pub last_delivery: Option<String>,
    /// HTTP status code returned by the most recent delivery attempt.
    #[serde(default)]
    pub last_status_code: Option<u16>,
}

fn default_webhook_type() -> String { "discord".to_string() }
fn default_true() -> bool { true }

fn webhooks_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    home.join(".config/jarvis/webhooks.json")
}

pub fn load_webhook_config() -> WebhookConfig {
    let path = webhooks_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_webhook_config(config: &WebhookConfig) -> Result<(), String> {
    let path = webhooks_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns true if `event_type` passes through the event_filter.
/// An empty filter means all events are allowed.
fn event_allowed(config: &WebhookConfig, event_type: &str) -> bool {
    config.event_filter.is_empty() || config.event_filter.iter().any(|f| f == event_type)
}

/// Synchronously POST a JSON body to a URL. Returns the HTTP status code and a
/// response snippet on success, or an error message on transport failure.
fn send_http_post(url: &str, body: &str) -> Result<(u16, String), String> {
    let response = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_string(body)
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let snippet: String = response
        .into_string()
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();

    if (200..300).contains(&status) {
        Ok((status, snippet))
    } else {
        Err(format!("Webhook returned HTTP {}: {}", status, snippet))
    }
}

/// Build the JSON body for a webhook notification.
fn build_body(webhook_type: &str, title: &str, message: &str, color: &str) -> String {
    let body = if webhook_type == "slack" {
        serde_json::json!({
            "text": format!("*{}*\n{}", title, message),
            "blocks": [{
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("*{}*\n{}", title, message)
                }
            }]
        })
    } else {
        let color_int = match color {
            "green" => 0x00ff88,
            "red" => 0xff3355,
            "yellow" => 0xffb74d,
            _ => 0x00d4ff,
        };
        serde_json::json!({
            "embeds": [{
                "title": title,
                "description": message,
                "color": color_int
            }]
        })
    };
    serde_json::to_string(&body).unwrap_or_default()
}

/// Payload emitted on the "webhook-failed" event.
#[derive(Debug, Clone, Serialize)]
struct WebhookFailedPayload {
    url: String,
    error: String,
    attempts: u32,
}

/// Send a webhook notification with exponential-backoff retry (non-blocking).
/// Attempts: up to 3. Delays between attempts: 1s, 4s, 16s.
/// If all attempts fail, emits a "webhook-failed" Tauri event when an AppHandle is provided.
pub fn send_notification_with_handle(
    title: &str,
    message: &str,
    color: &str,
    event_type: &str,
    app_handle: Option<tauri::AppHandle>,
) {
    let config = load_webhook_config();
    if !config.enabled || config.url.is_empty() {
        return;
    }
    if !event_allowed(&config, event_type) {
        return;
    }

    let url = config.url.clone();
    let body_str = build_body(&config.webhook_type, title, message, color);
    let event_type = event_type.to_string();

    // Use compare_exchange to atomically check-and-increment (avoids TOCTOU race)
    let mut current = ACTIVE_WEBHOOKS.load(Ordering::SeqCst);
    loop {
        if current >= MAX_CONCURRENT_WEBHOOKS {
            if let Some(ref handle) = app_handle {
                crate::app_logs::log_warn(handle, "Webhook rate limited, skipping".to_string());
            } else {
                log::warn!("Webhook rate limited, skipping");
            }
            return;
        }
        match ACTIVE_WEBHOOKS.compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }

    std::thread::spawn(move || {
        let _guard = WebhookGuard;

        const MAX_ATTEMPTS: u32 = 3;
        // Delays in seconds: 1, 4, 16 (exponential: 4^0, 4^1, 4^2)
        let delays_secs: [u64; 2] = [1, 4];

        let mut last_error = String::new();
        for attempt in 1..=MAX_ATTEMPTS {
            match send_http_post(&url, &body_str) {
                Ok((status, snippet)) => {
                    record_delivery(url.clone(), event_type.clone(), Some(status), true, snippet);
                    // Persist last delivery info to config
                    let mut cfg = load_webhook_config();
                    cfg.last_delivery = Some(chrono::Utc::now().to_rfc3339());
                    cfg.last_status_code = Some(status);
                    let _ = save_webhook_config(&cfg);
                    return; // success — done
                }
                Err(e) => {
                    last_error = e.clone();
                    log::warn!("Webhook attempt {}/{} failed: {}", attempt, MAX_ATTEMPTS, e);
                    if attempt < MAX_ATTEMPTS {
                        let delay = delays_secs
                            .get((attempt - 1) as usize)
                            .copied()
                            .unwrap_or(16);
                        std::thread::sleep(std::time::Duration::from_secs(delay));
                    }
                }
            }
        }

        // All attempts exhausted — record failed delivery
        record_delivery(url.clone(), event_type.clone(), None, false, last_error.clone());
        {
            let mut cfg = load_webhook_config();
            cfg.last_delivery = Some(chrono::Utc::now().to_rfc3339());
            cfg.last_status_code = None;
            let _ = save_webhook_config(&cfg);
        }

        // Emit event if we have a handle
        if let Some(handle) = app_handle {
            crate::app_logs::log_error(&handle, format!(
                "Webhook permanently failed after {} attempts: {}",
                MAX_ATTEMPTS,
                last_error
            ));
            let payload = WebhookFailedPayload {
                url: url.clone(),
                error: last_error,
                attempts: MAX_ATTEMPTS,
            };
            let _ = handle.emit("webhook-failed", payload);
        } else {
            log::warn!(
                "Webhook permanently failed after {} attempts: {}",
                MAX_ATTEMPTS,
                last_error
            );
        }
    });
}

/// Send a webhook notification (non-blocking). Callers without an AppHandle use this.
/// Retries with exponential backoff; failures are only logged (no event emitted).
pub fn send_notification(title: &str, message: &str, color: &str, event_type: &str) {
    send_notification_with_handle(title, message, color, event_type, None);
}

/// Called when a task completes
pub fn notify_task_complete(task_id: u64, target: &str, success: bool, output_preview: &str) {
    let config = load_webhook_config();
    if !config.enabled {
        return;
    }

    let preview: String = output_preview.chars().take(500).collect();

    if success && config.on_task_complete {
        send_notification(
            &format!("Tarea #{} completada", task_id),
            &format!("**Target:** {}\n```\n{}\n```", target, preview),
            "green",
            "task_complete",
        );
    } else if !success && config.on_task_fail {
        send_notification(
            &format!("Tarea #{} fallo", task_id),
            &format!("**Target:** {}\n```\n{}\n```", target, preview),
            "red",
            "task_error",
        );
    }
}

// Tauri commands

#[tauri::command]
pub fn get_webhook_config() -> WebhookConfig {
    load_webhook_config()
}

#[tauri::command]
pub fn save_webhook_settings(config: WebhookConfig) -> Result<bool, String> {
    save_webhook_config(&config)?;
    Ok(true)
}

#[tauri::command]
pub async fn test_webhook() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let config = load_webhook_config();
        if config.url.is_empty() {
            return Err("No webhook URL configured".to_string());
        }
        let body_str = build_body(&config.webhook_type, "JARVIS Test", "Webhook configurado correctamente!", "green");
        let (status, snippet) = send_http_post(&config.url, &body_str)?;
        record_delivery(config.url.clone(), "test".to_string(), Some(status), true, snippet);
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Returns the last up to 20 webhook delivery attempts (newest last).
#[tauri::command]
pub fn get_webhook_deliveries() -> Vec<WebhookDelivery> {
    DELIVERY_LOG.lock().map(|log| log.clone()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(enabled: bool, filter: Vec<&str>) -> WebhookConfig {
        WebhookConfig {
            enabled,
            url: "http://example.com/hook".into(),
            webhook_type: "discord".into(),
            on_task_complete: true,
            on_task_fail: true,
            on_pipeline_complete: false,
            event_filter: filter.into_iter().map(str::to_string).collect(),
            last_delivery: None,
            last_status_code: None,
        }
    }

    #[test]
    fn test_event_allowed_empty_filter_allows_all() {
        let cfg = make_config(true, vec![]);
        assert!(event_allowed(&cfg, "task_complete"));
        assert!(event_allowed(&cfg, "pipeline_done"));
        assert!(event_allowed(&cfg, "anything"));
    }

    #[test]
    fn test_event_allowed_with_filter() {
        let cfg = make_config(true, vec!["task_complete", "task_error"]);
        assert!(event_allowed(&cfg, "task_complete"));
        assert!(event_allowed(&cfg, "task_error"));
        assert!(!event_allowed(&cfg, "pipeline_done"));
        assert!(!event_allowed(&cfg, "test"));
    }

    #[test]
    fn test_webhook_config_default() {
        let cfg = WebhookConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.url.is_empty());
        assert!(cfg.event_filter.is_empty());
        assert!(cfg.last_delivery.is_none());
        assert!(cfg.last_status_code.is_none());
    }

    #[test]
    fn test_webhook_delivery_record_and_retrieve() {
        // Clear existing log entries that might come from other tests
        if let Ok(mut log) = DELIVERY_LOG.lock() {
            log.clear();
        }

        record_delivery(
            "http://test.example/hook".into(),
            "task_complete".into(),
            Some(200),
            true,
            "ok".into(),
        );

        let deliveries = get_webhook_deliveries();
        assert!(!deliveries.is_empty());
        let last = deliveries.last().unwrap();
        assert_eq!(last.event_type, "task_complete");
        assert_eq!(last.status_code, Some(200));
        assert!(last.success);
        assert!(!last.timestamp.is_empty());
    }

    #[test]
    fn test_delivery_log_capped_at_max() {
        if let Ok(mut log) = DELIVERY_LOG.lock() {
            log.clear();
        }

        for i in 0..(MAX_DELIVERY_LOG + 5) {
            record_delivery(
                "http://example.com".into(),
                format!("event_{}", i),
                Some(200),
                true,
                String::new(),
            );
        }

        let deliveries = get_webhook_deliveries();
        assert_eq!(deliveries.len(), MAX_DELIVERY_LOG);
        // The oldest entries should have been dropped; last entry is the most recent
        assert_eq!(deliveries.last().unwrap().event_type, format!("event_{}", MAX_DELIVERY_LOG + 4));
    }

    #[test]
    fn test_build_body_discord_color_mapping() {
        let green = build_body("discord", "Title", "Msg", "green");
        let parsed: serde_json::Value = serde_json::from_str(&green).unwrap();
        let color = parsed["embeds"][0]["color"].as_u64().unwrap();
        assert_eq!(color, 0x00ff88);

        let red = build_body("discord", "Title", "Msg", "red");
        let parsed: serde_json::Value = serde_json::from_str(&red).unwrap();
        let color = parsed["embeds"][0]["color"].as_u64().unwrap();
        assert_eq!(color, 0xff3355);
    }
}
