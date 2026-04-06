use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

pub struct AppLogs {
    pub entries: Mutex<VecDeque<AppLogEntry>>,
}

impl AppLogs {
    pub fn new() -> Self {
        Self { entries: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, level: &str, message: String) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.push_back(AppLogEntry {
            level: level.to_string(),
            message,
            timestamp: Utc::now().to_rfc3339(),
        });
        // Keep last 500 entries
        while entries.len() > 500 {
            entries.pop_front();
        }
    }
}

#[tauri::command]
pub fn get_app_logs(
    app_logs: tauri::State<'_, AppLogs>,
    since: Option<String>,
) -> Vec<AppLogEntry> {
    let entries = app_logs.entries.lock().unwrap_or_else(|e| e.into_inner());
    match since {
        None => entries.iter().cloned().collect::<Vec<_>>(),
        Some(ts) => entries.iter().filter(|e| e.timestamp > ts).cloned().collect::<Vec<_>>(),
    }
}

#[tauri::command]
pub fn clear_app_logs(app_logs: tauri::State<'_, AppLogs>) {
    let mut entries = app_logs.entries.lock().unwrap_or_else(|e| e.into_inner());
    entries.clear();
}

#[tauri::command]
pub fn get_app_log_stats(app_logs: tauri::State<'_, AppLogs>) -> serde_json::Value {
    let entries = app_logs.entries.lock().unwrap_or_else(|e| e.into_inner());
    let total = entries.len();
    let mut by_level: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        *by_level.entry(entry.level.clone()).or_insert(0) += 1;
    }
    let oldest_ts = entries.front().map(|e| e.timestamp.clone());
    serde_json::json!({
        "total": total,
        "by_level": by_level,
        "oldest_ts": oldest_ts,
    })
}

pub fn log_warn(app: &tauri::AppHandle, message: String) {
    use tauri::Manager;
    log::warn!("{}", message);
    if let Some(logs) = app.try_state::<AppLogs>() {
        logs.push("warn", message);
    }
}

pub fn log_error(app: &tauri::AppHandle, message: String) {
    use tauri::Manager;
    log::error!("{}", message);
    if let Some(logs) = app.try_state::<AppLogs>() {
        logs.push("error", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_logs_with(entries: &[(&str, &str)]) -> AppLogs {
        let logs = AppLogs::new();
        for (level, msg) in entries {
            logs.push(level, msg.to_string());
        }
        logs
    }

    #[test]
    fn test_push_and_count() {
        let logs = make_logs_with(&[("info", "a"), ("warn", "b"), ("error", "c")]);
        let entries = logs.entries.lock().unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_clear_empties_buffer() {
        let logs = make_logs_with(&[("info", "x"), ("debug", "y")]);
        {
            let mut entries = logs.entries.lock().unwrap();
            entries.clear();
        }
        let entries = logs.entries.lock().unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_stats_by_level() {
        let logs = make_logs_with(&[
            ("info", "one"),
            ("info", "two"),
            ("warn", "three"),
            ("error", "four"),
        ]);
        let entries = logs.entries.lock().unwrap();
        let total = entries.len();
        let mut by_level: HashMap<String, usize> = HashMap::new();
        for e in entries.iter() {
            *by_level.entry(e.level.clone()).or_insert(0) += 1;
        }
        assert_eq!(total, 4);
        assert_eq!(by_level["info"], 2);
        assert_eq!(by_level["warn"], 1);
        assert_eq!(by_level["error"], 1);
    }

    #[test]
    fn test_buffer_cap_at_500() {
        let logs = AppLogs::new();
        for i in 0..600 {
            logs.push("debug", format!("msg {}", i));
        }
        let entries = logs.entries.lock().unwrap();
        assert_eq!(entries.len(), 500);
        // Oldest entry should be msg 100 (first 100 were evicted)
        assert_eq!(entries.front().unwrap().message, "msg 100");
    }
}
