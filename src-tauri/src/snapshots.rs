use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tauri::Emitter;

use crate::config;
use crate::machines::MachineRegistry;
use crate::tasks::TaskStore;
use crate::types::shell_escape;

fn validate_name(name: &str) -> Result<(), String> {
    if name.len() > 100 {
        return Err("Name too long (max 100 chars)".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Name must contain only alphanumeric, dash, or underscore characters".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub name: String,
    pub created_at: String,
    pub objetivo: String,
    pub rama: String,
    pub session_id: String,
    pub branches: Vec<BranchSnapshot>,
    pub pending_tasks: Vec<String>,
    pub machine_count: usize,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSnapshot {
    pub repo_name: String,
    pub branch: String,
    pub last_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub name: String,
    pub created_at: String,
    pub objetivo: String,
    pub rama: String,
}

fn snapshots_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = home.join(".config/jarvis/snapshots");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn get_branch_info(host: &str, repo_path: &str, repo_name: &str) -> BranchSnapshot {
    let (branch, last_commit) = if host == "local" {
        let branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let commit = Command::new("git")
            .args(["log", "-1", "--oneline"])
            .current_dir(repo_path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        (branch, commit)
    } else {
        let script = format!(
            "cd {} && echo \"$(git rev-parse --abbrev-ref HEAD)|||$(git log -1 --oneline)\"",
            shell_escape(repo_path)
        );
        let raw = Command::new("ssh")
            .args([
                "-o", "ConnectTimeout=5",
                "-o", "ServerAliveInterval=5",
                "-o", "ServerAliveCountMax=3",
                host,
                &script,
            ])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if let Some((b, c)) = raw.split_once("|||") {
            (b.to_string(), c.to_string())
        } else {
            (String::new(), String::new())
        }
    };

    BranchSnapshot {
        repo_name: repo_name.to_string(),
        branch,
        last_commit,
    }
}

fn save_snapshot(
    name: &str,
    store: &TaskStore,
    registry: &MachineRegistry,
) -> Result<SessionSnapshot, String> {
    let cfg = config::load_config();
    let now = chrono::Utc::now().to_rfc3339();

    // Snapshot needed data inside the lock, then drop before doing I/O
    let (machine_count, repo_infos) = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let count = machines.values().filter(|m| m.enabled).count();
        let infos: Vec<(String, String, String)> = machines
            .values()
            .filter(|m| m.enabled)
            .flat_map(|m| {
                m.repos
                    .iter()
                    .map(|r| (m.host.clone(), r.path.clone(), r.name.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        (count, infos)
    };

    // Do all I/O outside the lock, in parallel
    let handles: Vec<_> = repo_infos
        .iter()
        .map(|(host, path, name)| {
            let host = host.clone();
            let path = path.clone();
            let name = name.clone();
            std::thread::spawn(move || get_branch_info(&host, &path, &name))
        })
        .collect();
    let mut branches = Vec::new();
    for handle in handles {
        if let Ok(b) = handle.join() {
            branches.push(b);
        }
    }

    // Gather pending (running) task prompts
    let pending_tasks: Vec<String> = {
        let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .filter(|t| t.status == "running")
            .map(|t| t.prompt.clone())
            .collect()
    };

    let snapshot = SessionSnapshot {
        name: name.to_string(),
        created_at: now,
        objetivo: cfg.session.objetivo.clone(),
        rama: cfg.session.rama.clone(),
        session_id: cfg.session.id.clone(),
        branches,
        pending_tasks,
        machine_count,
        description: None,
        tags: Vec::new(),
    };

    let path = snapshots_dir().join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;

    Ok(snapshot)
}

fn list_snapshots() -> Vec<SnapshotSummary> {
    let dir = snapshots_dir();
    let mut summaries = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(snap) = serde_json::from_str::<SessionSnapshot>(&data) {
                        summaries.push(SnapshotSummary {
                            name: snap.name,
                            created_at: snap.created_at,
                            objetivo: snap.objetivo,
                            rama: snap.rama,
                        });
                    }
                }
            }
        }
    }

    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    summaries
}

fn restore_snapshot(
    name: &str,
    registry: &MachineRegistry,
) -> Result<SessionSnapshot, String> {
    let path = snapshots_dir().join(format!("{}.json", name));
    let data = fs::read_to_string(&path).map_err(|e| format!("No se pudo leer el snapshot: {}", e))?;
    let snapshot: SessionSnapshot =
        serde_json::from_str(&data).map_err(|e| format!("Snapshot corrupto: {}", e))?;

    // Update config with snapshot's session data
    let mut cfg = config::load_config();
    cfg.session.id = snapshot.session_id.clone();
    cfg.session.rama = snapshot.rama.clone();
    cfg.session.objetivo = snapshot.objetivo.clone();
    config::save_config(&cfg)?;

    // Reload registry from updated config
    registry.reload_from_config();

    Ok(snapshot)
}

fn delete_snapshot(name: &str) -> Result<bool, String> {
    let path = snapshots_dir().join(format!("{}.json", name));
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_session_snapshot(
    name: String,
    store: tauri::State<'_, TaskStore>,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<SessionSnapshot, String> {
    validate_name(&name)?;
    save_snapshot(&name, &store, &registry)
}

#[tauri::command]
pub fn list_session_snapshots() -> Vec<SnapshotSummary> {
    list_snapshots()
}

#[tauri::command]
pub fn restore_session_snapshot(
    name: String,
    _store: tauri::State<'_, TaskStore>,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<SessionSnapshot, String> {
    validate_name(&name)?;
    restore_snapshot(&name, &registry)
}

#[tauri::command]
pub fn delete_session_snapshot(name: String) -> Result<bool, String> {
    validate_name(&name)?;
    delete_snapshot(&name)
}

// ---------------------------------------------------------------------------
// Workspace commands (aliases over the snapshot system)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_workspaces() -> Vec<SnapshotSummary> {
    list_snapshots()
}

#[tauri::command]
pub fn save_workspace(
    name: String,
    store: tauri::State<'_, TaskStore>,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<SessionSnapshot, String> {
    validate_name(&name)?;
    save_snapshot(&name, &store, &registry)
}

#[tauri::command]
pub fn switch_workspace(
    name: String,
    registry: tauri::State<'_, MachineRegistry>,
    app: tauri::AppHandle,
) -> Result<SessionSnapshot, String> {
    validate_name(&name)?;
    let snapshot = restore_snapshot(&name, &registry)?;
    // Emit event so frontend can react
    let _ = app.emit("workspace-switched", serde_json::json!({
        "name": name,
        "snapshot": snapshot,
    }));
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// Search & Tag commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn search_snapshots(query: String) -> Vec<SessionSnapshot> {
    let dir = snapshots_dir();
    let q = query.to_lowercase();
    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(snap) = serde_json::from_str::<SessionSnapshot>(&data) {
                    let matches_name = snap.name.to_lowercase().contains(&q);
                    let matches_description = snap
                        .description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false);
                    let matches_tag = snap.tags.iter().any(|t| t.to_lowercase().contains(&q));
                    if matches_name || matches_description || matches_tag {
                        results.push(snap);
                    }
                }
            }
        }
    }

    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    results
}

#[tauri::command]
pub fn tag_snapshot(id: String, tags: Vec<String>) -> Result<(), String> {
    validate_name(&id)?;
    let path = snapshots_dir().join(format!("{}.json", id));
    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Snapshot not found: {}", e))?;
    let mut snap: SessionSnapshot =
        serde_json::from_str(&data).map_err(|e| format!("Snapshot corrupto: {}", e))?;
    snap.tags = tags;
    let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_snapshots_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis_snap_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn write_snap(dir: &PathBuf, snap: &SessionSnapshot) {
        let json = serde_json::to_string_pretty(snap).unwrap();
        fs::write(dir.join(format!("{}.json", snap.name)), json).unwrap();
    }

    fn sample_snap(name: &str) -> SessionSnapshot {
        SessionSnapshot {
            name: name.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            objetivo: "test objective".to_string(),
            rama: "main".to_string(),
            session_id: "sess-1".to_string(),
            branches: vec![],
            pending_tasks: vec![],
            machine_count: 1,
            description: None,
            tags: vec![],
        }
    }

    #[test]
    fn test_validate_name_ok() {
        assert!(validate_name("my-snapshot_01").is_ok());
    }

    #[test]
    fn test_validate_name_rejects_spaces() {
        assert!(validate_name("bad name").is_err());
    }

    #[test]
    fn test_validate_name_rejects_long() {
        let long = "a".repeat(101);
        assert!(validate_name(&long).is_err());
    }

    #[test]
    fn test_snapshot_roundtrip_with_new_fields() {
        let mut snap = sample_snap("roundtrip");
        snap.description = Some("my description".to_string());
        snap.tags = vec!["prod".to_string(), "release".to_string()];

        let json = serde_json::to_string(&snap).unwrap();
        let decoded: SessionSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.description.as_deref(), Some("my description"));
        assert_eq!(decoded.tags, vec!["prod", "release"]);
    }

    #[test]
    fn test_snapshot_deserializes_missing_new_fields() {
        // Old snapshot JSON without description/tags — should deserialize with defaults.
        let old_json = r#"{
            "name": "old",
            "createdAt": "2025-01-01T00:00:00Z",
            "objetivo": "obj",
            "rama": "main",
            "sessionId": "s1",
            "branches": [],
            "pendingTasks": [],
            "machineCount": 0
        }"#;
        let snap: SessionSnapshot = serde_json::from_str(old_json).unwrap();
        assert!(snap.description.is_none());
        assert!(snap.tags.is_empty());
    }

    #[test]
    fn test_search_snapshots_matches_tag() {
        // Build a minimal snapshot with a known tag and check the search logic inline.
        let mut snap = sample_snap("tagged-snap");
        snap.tags = vec!["backend".to_string()];

        let q = "backend".to_lowercase();
        let matches_tag = snap.tags.iter().any(|t| t.to_lowercase().contains(&q));
        assert!(matches_tag);

        let q2 = "frontend".to_lowercase();
        let no_match = snap.tags.iter().any(|t| t.to_lowercase().contains(&q2));
        assert!(!no_match);
    }

    #[test]
    fn test_tag_snapshot_persists() {
        let dir = temp_snapshots_dir();
        let snap = sample_snap("persist-test");
        write_snap(&dir, &snap);

        // Read back and mutate tags manually (mirrors tag_snapshot logic).
        let path = dir.join("persist-test.json");
        let data = fs::read_to_string(&path).unwrap();
        let mut loaded: SessionSnapshot = serde_json::from_str(&data).unwrap();
        loaded.tags = vec!["gpu".to_string(), "linux".to_string()];
        let json = serde_json::to_string_pretty(&loaded).unwrap();
        fs::write(&path, json).unwrap();

        let reread: SessionSnapshot =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reread.tags, vec!["gpu", "linux"]);

        // cleanup
        let _ = fs::remove_dir_all(&dir);
    }
}
