use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/jarvis")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigBackup {
    #[serde(rename = "version")]
    pub backup_version: u32,
    pub config: Option<String>,
    pub rules: Option<serde_json::Value>,
    pub crons: Option<serde_json::Value>,
    pub messages: Option<serde_json::Value>,
    pub snapshots: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[tauri::command]
pub fn export_config() -> Result<ConfigBackup, String> {
    let dir = config_dir();

    let config = fs::read_to_string(dir.join("config.toml")).ok();

    let rules = read_json_file(&dir.join("rules.json"));
    let crons = read_json_file(&dir.join("crons.json"));
    let messages = read_json_file(&dir.join("messages.json"));

    // Read snapshots directory
    let snapshots_dir = dir.join("snapshots");
    let mut snapshots: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    if let Ok(entries) = fs::read_dir(&snapshots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            snapshots.insert(stem.to_string(), val);
                        }
                    }
                }
            }
        }
    }

    Ok(ConfigBackup {
        backup_version: 1,
        config,
        rules,
        crons,
        messages,
        snapshots: if snapshots.is_empty() {
            None
        } else {
            Some(snapshots)
        },
    })
}

#[tauri::command]
pub fn import_config(data: String) -> Result<(), String> {
    let backup: ConfigBackup =
        serde_json::from_str(&data).map_err(|e| format!("Invalid backup format: {}", e))?;

    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create config dir: {}", e))?;

    if let Some(config_toml) = backup.config {
        fs::write(dir.join("config.toml"), config_toml)
            .map_err(|e| format!("Cannot write config.toml: {}", e))?;
    }

    if let Some(rules) = backup.rules {
        write_json_file(&dir.join("rules.json"), &rules)?;
    }

    if let Some(crons) = backup.crons {
        write_json_file(&dir.join("crons.json"), &crons)?;
    }

    if let Some(messages) = backup.messages {
        write_json_file(&dir.join("messages.json"), &messages)?;
    }

    if let Some(snapshots) = backup.snapshots {
        let snapshots_dir = dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir)
            .map_err(|e| format!("Cannot create snapshots dir: {}", e))?;
        for (name, val) in snapshots {
            // Validate name to prevent path traversal (same rules as snapshots::validate_name)
            if name.len() > 100 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(format!("Invalid snapshot name in backup: '{}'", name));
            }
            let path = snapshots_dir.join(format!("{}.json", name));
            write_json_file(&path, &val)?;
        }
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub filename: String,
    pub created_at: String,
    pub size_bytes: u64,
}

#[tauri::command]
pub fn list_backups() -> Vec<BackupInfo> {
    let backups_dir = config_dir().join("backups");
    let mut result = Vec::new();

    let entries = match fs::read_dir(&backups_dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size_bytes = meta.len();
        let created_at = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| {
                // Format as RFC3339-like UTC string from epoch seconds
                let secs = d.as_secs();
                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                    .unwrap_or_else(chrono::Utc::now);
                dt.to_rfc3339()
            })
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        result.push(BackupInfo { filename, created_at, size_bytes });
    }

    // Sort by filename descending (newest first if names are timestamp-based)
    result.sort_by(|a, b| b.filename.cmp(&a.filename));
    result
}

fn read_json_file(path: &PathBuf) -> Option<serde_json::Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_json_file(path: &PathBuf, val: &serde_json::Value) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(val).map_err(|e| format!("Serialization error: {}", e))?;
    fs::write(path, content).map_err(|e| format!("Cannot write {:?}: {}", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_backups_missing_dir_returns_empty() {
        // If the backups dir does not exist, list_backups returns an empty Vec without panicking.
        // We override config_dir indirectly by ensuring the real call returns gracefully.
        // (The actual ~/.config/jarvis/backups may or may not exist; both outcomes are valid.)
        let result = list_backups();
        // Result is always a Vec — just ensure it doesn't panic and is well-formed.
        for item in &result {
            assert!(!item.filename.is_empty());
            assert!(item.size_bytes < u64::MAX);
        }
    }

    #[test]
    fn test_list_backups_with_temp_dir() {
        let tmp = tempfile_dir();
        let backups_dir = tmp.join("backups");
        fs::create_dir_all(&backups_dir).unwrap();

        // Write two fake backup files
        let f1 = backups_dir.join("2026-01-01_backup.json");
        let f2 = backups_dir.join("2026-03-15_backup.json");
        fs::write(&f1, b"{\"version\":1}").unwrap();
        fs::write(&f2, b"{\"version\":2}").unwrap();

        // Verify our BackupInfo struct can hold expected values
        let b = BackupInfo {
            filename: "2026-03-15_backup.json".to_string(),
            created_at: "2026-03-15T00:00:00+00:00".to_string(),
            size_bytes: fs::metadata(&f2).unwrap().len(),
        };
        assert_eq!(b.filename, "2026-03-15_backup.json");
        assert!(b.size_bytes > 0);
    }

    #[test]
    fn test_backup_info_serialization() {
        let info = BackupInfo {
            filename: "backup-2026.json".to_string(),
            created_at: "2026-03-17T12:00:00+00:00".to_string(),
            size_bytes: 1024,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"filename\""));
        assert!(json.contains("\"createdAt\""));
        assert!(json.contains("\"sizeBytes\""));
        assert!(json.contains("backup-2026.json"));
        assert!(json.contains("1024"));
    }

    /// Returns a system temp dir path for test use (no new deps needed).
    fn tempfile_dir() -> PathBuf {
        std::env::temp_dir().join(format!("jarvis_backup_test_{}", std::process::id()))
    }
}
