use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

// ─── Notification prefs path ────────────────────────────────────────────────

fn prefs_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".config/jarvis/notif_prefs.json")
}

// ─── NotificationPrefs ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationPrefs {
    pub enabled: bool,
    pub sound: bool,
    pub task_complete: bool,
    pub task_error: bool,
    pub machine_offline: bool,
    pub cron_fired: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            sound: true,
            task_complete: true,
            task_error: true,
            machine_offline: true,
            cron_fired: false,
        }
    }
}

#[tauri::command]
pub fn get_notification_prefs() -> NotificationPrefs {
    let path = prefs_path();
    if path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(prefs) = serde_json::from_str::<NotificationPrefs>(&raw) {
                return prefs;
            }
        }
    }
    NotificationPrefs::default()
}

#[tauri::command]
pub fn save_notification_prefs(prefs: NotificationPrefs) -> Result<(), String> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

// ─── NotifHistoryEntry & ring buffer ────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotifHistoryEntry {
    pub id: u64,
    pub timestamp: String,
    pub title: String,
    pub body: String,
    pub level: String,
}

struct NotifRingBuffer {
    buf: VecDeque<NotifHistoryEntry>,
    next_id: u64,
}

impl NotifRingBuffer {
    fn new() -> Self {
        Self {
            buf: VecDeque::with_capacity(50),
            next_id: 1,
        }
    }

    fn push(&mut self, title: String, body: String, level: String) {
        let entry = NotifHistoryEntry {
            id: self.next_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            title,
            body,
            level,
        };
        self.next_id += 1;
        if self.buf.len() == 50 {
            self.buf.pop_front();
        }
        self.buf.push_back(entry);
    }

    fn last_50(&self) -> Vec<NotifHistoryEntry> {
        self.buf.iter().cloned().collect()
    }
}

static NOTIF_HISTORY: OnceLock<Mutex<NotifRingBuffer>> = OnceLock::new();

fn history() -> &'static Mutex<NotifRingBuffer> {
    NOTIF_HISTORY.get_or_init(|| Mutex::new(NotifRingBuffer::new()))
}

#[tauri::command]
pub fn get_notification_history() -> Vec<NotifHistoryEntry> {
    let lock = history().lock().unwrap_or_else(|p| p.into_inner());
    lock.last_50()
}

// ─── Notification level ──────────────────────────────────────────────────────

/// Classify a notification category so prefs can gate it.
#[allow(dead_code)]
pub enum NotifLevel {
    TaskComplete,
    TaskError,
    MachineOffline,
    CronFired,
    Info,
}

impl NotifLevel {
    fn as_str(&self) -> &'static str {
        match self {
            NotifLevel::TaskComplete => "task_complete",
            NotifLevel::TaskError => "task_error",
            NotifLevel::MachineOffline => "machine_offline",
            NotifLevel::CronFired => "cron_fired",
            NotifLevel::Info => "info",
        }
    }

    fn allowed_by(&self, prefs: &NotificationPrefs) -> bool {
        if !prefs.enabled {
            return false;
        }
        match self {
            NotifLevel::TaskComplete => prefs.task_complete,
            NotifLevel::TaskError => prefs.task_error,
            NotifLevel::MachineOffline => prefs.machine_offline,
            NotifLevel::CronFired => prefs.cron_fired,
            NotifLevel::Info => true,
        }
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Send a native OS notification filtered by user prefs; always records to history.
pub fn send_desktop_notification(
    app: &AppHandle,
    title: &str,
    body: &str,
    level: NotifLevel,
) {
    // Always record to history regardless of prefs
    {
        let mut lock = history().lock().unwrap_or_else(|p| p.into_inner());
        lock.push(title.to_owned(), body.to_owned(), level.as_str().to_owned());
    }

    let prefs = get_notification_prefs();
    if !level.allowed_by(&prefs) {
        return;
    }

    let _ = app.notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

/// Convenience wrapper kept for callers that don't specify a level (defaults to Info).
pub fn send_native(app: &AppHandle, title: &str, body: &str) {
    send_desktop_notification(app, title, body, NotifLevel::Info);
}

// ─── Legacy commands (kept for backwards compat) ─────────────────────────────

#[tauri::command]
pub fn get_notifications_enabled() -> bool {
    get_notification_prefs().enabled
}

#[tauri::command]
pub fn set_notifications_enabled(enabled: bool) -> bool {
    let mut prefs = get_notification_prefs();
    prefs.enabled = enabled;
    save_notification_prefs(prefs).is_ok()
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_prefs() -> NotificationPrefs {
        NotificationPrefs::default()
    }

    #[test]
    fn test_default_prefs_enabled() {
        let p = default_prefs();
        assert!(p.enabled);
        assert!(p.sound);
        assert!(p.task_complete);
        assert!(p.task_error);
        assert!(p.machine_offline);
        assert!(!p.cron_fired);
    }

    #[test]
    fn test_level_blocked_when_disabled() {
        let mut p = default_prefs();
        p.enabled = false;
        assert!(!NotifLevel::TaskComplete.allowed_by(&p));
        assert!(!NotifLevel::Info.allowed_by(&p));
    }

    #[test]
    fn test_cron_blocked_by_default() {
        let p = default_prefs();
        assert!(!NotifLevel::CronFired.allowed_by(&p));
    }

    #[test]
    fn test_cron_allowed_when_opted_in() {
        let mut p = default_prefs();
        p.cron_fired = true;
        assert!(NotifLevel::CronFired.allowed_by(&p));
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let mut rb = NotifRingBuffer::new();
        for i in 0..60u64 {
            rb.push(format!("title-{i}"), "body".into(), "info".into());
        }
        let entries = rb.last_50();
        assert_eq!(entries.len(), 50);
        // Oldest should have been evicted; first kept is title-10
        assert_eq!(entries[0].title, "title-10");
    }

    #[test]
    fn test_ring_buffer_ids_monotonically_increase() {
        let mut rb = NotifRingBuffer::new();
        rb.push("a".into(), "b".into(), "info".into());
        rb.push("c".into(), "d".into(), "info".into());
        let entries = rb.last_50();
        assert_eq!(entries.len(), 2);
        assert!(entries[1].id > entries[0].id);
    }

    #[test]
    fn test_prefs_roundtrip_json() {
        let p = NotificationPrefs {
            enabled: false,
            sound: false,
            task_complete: true,
            task_error: false,
            machine_offline: true,
            cron_fired: true,
        };
        let json = serde_json::to_string(&p).unwrap();
        let p2: NotificationPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.enabled, p.enabled);
        assert_eq!(p2.cron_fired, p.cron_fired);
        assert_eq!(p2.machine_offline, p.machine_offline);
    }
}
