use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Manager;

use tauri::{AppHandle, Emitter};

use crate::jsonl::*;
use crate::machines::MachineRegistry;
use crate::tasks::TaskStore;
use crate::types::{shell_escape, Activity, AgentDetail, AgentInfo, LogEntry, RoundInfo, RoundSummary, SessionData};

// ---------------------------------------------------------------------------
// Session directory discovery
// ---------------------------------------------------------------------------

pub fn find_latest_session() -> Option<String> {
    let base = "/tmp/jarvis-collab/";
    let entries = fs::read_dir(base).ok()?;
    let mut dirs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("parallel-") || name.starts_with("longrun-") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    dirs.sort();
    dirs.reverse();
    dirs.first().map(|d| format!("{}{}", base, d))
}

// ---------------------------------------------------------------------------
// Full session data
// ---------------------------------------------------------------------------

pub fn get_session_data_full(store: &TaskStore, registry: Option<&MachineRegistry>) -> SessionData {
    let config = store.config.lock().unwrap_or_else(|e| e.into_inner()).clone();
    get_session_data_with_config(config, registry)
}

fn get_session_data_with_config(config: crate::types::Config, registry: Option<&MachineRegistry>) -> SessionData {
    let session_dir = find_latest_session();

    let (mut session_id, mut objetivo, mut rama, total_rounds) = if let Some(ref dir) = session_dir
    {
        let sid = Path::new(dir)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let chat_log = fs::read_to_string(format!("{}/chat.log", dir)).unwrap_or_default();

        let obj = extract_match(&chat_log, r"Objetivo: (.+)")
            .unwrap_or_default()
            .trim()
            .to_string();
        let ram = extract_match(&chat_log, r"Rama: (\S+)")
            .unwrap_or_default()
            .trim()
            .to_string();
        let rounds = extract_match(&chat_log, r"Rondas: (\d+)")
            .unwrap_or_else(|| "?".to_string());

        (sid, obj, ram, rounds)
    } else {
        (String::new(), String::new(), String::new(), "?".to_string())
    };

    // Apply config overrides
    if !config.session_id.is_empty() {
        session_id = config.session_id;
    }
    if !config.rama.is_empty() {
        rama = config.rama;
    }
    if !config.objetivo.is_empty() {
        objetivo = config.objetivo;
    }

    // Check if agents are running — only match JARVIS-spawned non-interactive claude (-p/--print)
    let ps = run_cmd("ps", &["aux"]);
    let atlas_running = ps.lines().any(|line| {
        !line.contains("grep") && !line.contains("ssh") &&
        (line.contains(" claude -p ") || line.contains(" claude --print ") || line.ends_with(" claude --print"))
    });

    // For pixel: check if an SSH session to pixel is running claude -p
    let remote_host_name = registry.and_then(|reg| {
        let machines = reg.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.values().find(|m| m.host != "local" && m.enabled).map(|m| m.host.clone())
    });
    let pixel_running = if let Some(ref rhost) = remote_host_name {
        // Must be a single ps line that contains both the host AND claude -p (task execution SSH)
        ps.lines().any(|line| {
            !line.contains("grep") &&
            line.contains("ssh") && line.contains(rhost) &&
            (line.contains("claude -p") || line.contains("claude --print"))
        })
    } else {
        false
    };

    // Parse rounds
    let mut rounds_info = Vec::new();
    let mut round_summaries = Vec::new();
    if let Some(ref dir) = session_dir {
        if let Ok(entries) = fs::read_dir(dir) {
            let mut round_files: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("round-") && name.ends_with(".md") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();
            round_files.sort();

            for f in &round_files {
                let path = format!("{}/{}", dir, f);
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let done = Path::new(&format!("{}.done", path)).exists();
                rounds_info.push(RoundInfo {
                    file: f.clone(),
                    size,
                    done,
                });

                if let Ok(content) = fs::read_to_string(&path) {
                    let first_line = content
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>();
                    round_summaries.push(RoundSummary {
                        file: f.clone(),
                        summary: first_line,
                        size: content.len(),
                    });
                }
            }
        }
    }

    // Git commits — use registry to find local/remote repo paths
    let mut commits_back = Vec::new();
    let mut commits_front = Vec::new();

    // Extract machine info from registry (with lock released before I/O)
    let (local_repo_path, remote_commit_info) = if let Some(reg) = registry {
        let machines = reg.machines.lock().unwrap_or_else(|e| e.into_inner());
        let local_machine = machines.values().find(|m| m.host == "local" && m.enabled);
        let local_rp = local_machine
            .and_then(|m| m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()));
        let remote_machine = machines.values().find(|m| m.host != "local" && m.enabled);
        let remote_info = remote_machine.map(|m| {
            let repo = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
            (m.host.clone(), repo)
        });
        (local_rp, remote_info)
    } else {
        (None, None)
    };

    if !rama.is_empty() {
        if let Some(ref local_rp) = local_repo_path {
            let cb = Command::new("git")
                .args(["log", "--oneline", &rama, "--not", "main"])
                .current_dir(local_rp)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            commits_back = cb
                .lines()
                .filter(|l| !l.is_empty())
                .take(10)
                .map(|s| s.to_string())
                .collect();
        }

        if let Some((ref rhost, ref rrepo)) = remote_commit_info {
            let cf = run_cmd(
                "ssh",
                &[
                    "-o",
                    "ConnectTimeout=3",
                    "-o",
                    "ServerAliveInterval=5",
                    "-o",
                    "ServerAliveCountMax=3",
                    rhost,
                    &format!(
                        "cd {} && git log --oneline {} --not master 2>/dev/null | head -10",
                        shell_escape(rrepo), shell_escape(&rama)
                    ),
                ],
            );
            commits_front = cf
                .lines()
                .filter(|l| !l.is_empty())
                .take(10)
                .map(|s| s.to_string())
                .collect();
        }
    }

    SessionData {
        active: atlas_running || pixel_running,
        session_id,
        objetivo,
        rama,
        total_rounds,
        atlas_running,
        pixel_running,
        rounds: rounds_info,
        round_summaries,
        commits_back,
        commits_front,
    }
}

fn extract_match(text: &str, pattern: &str) -> Option<String> {
    let re = regex_lite::Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

// ---------------------------------------------------------------------------
// Background monitor
// ---------------------------------------------------------------------------

pub fn start_monitor(app: AppHandle, shutdown: Arc<AtomicBool>) {
    log::info!("Session monitor started");
    let app_session = app.clone();
    let app_commits = app.clone();

    // Shared cache for remote SSH data (written by remote thread, read by main thread)
    struct RemoteCache {
        pixel_activity: Vec<Activity>,
        pixel_agent_info: AgentInfo,
    }
    let remote_cache = Arc::new(Mutex::new(RemoteCache {
        pixel_activity: vec![],
        pixel_agent_info: AgentInfo { agent_count: 0, skills: vec![] },
    }));

    // Remote SSH polling thread (every 8s, non-blocking to main loop)
    let remote_cache_writer = remote_cache.clone();
    let app_remote = app.clone();
    let shutdown_remote = shutdown.clone();
    thread::spawn(move || {
        let mut consecutive_failures: u32 = 0;
        loop {
            let poll_interval = if consecutive_failures >= 5 { 30 } else { 8 };
            thread::sleep(Duration::from_secs(poll_interval));
            if shutdown_remote.load(Ordering::Relaxed) { break; }
            let registry = app_remote.state::<MachineRegistry>();
            let remote_info = {
                let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
                let remote_machine = machines.values().find(|m| m.host != "local" && m.enabled);
                remote_machine.map(|m| {
                    let repo_path = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
                    (m.host.clone(), repo_path_to_jsonl_dir(&repo_path, false, m.home_dir.as_deref()))
                })
            };

            if let Some((ref host, ref jsonl_dir)) = remote_info {
                let ssh_cmd = format!(
                    "JSONL=$(ls -t {}*.jsonl 2>/dev/null | head -1); tail -80 \"$JSONL\" 2>/dev/null",
                    shell_escape(jsonl_dir)
                );
                let ssh_args: Vec<&str> = vec![
                    "-o", "ConnectTimeout=3",
                    "-o", "ServerAliveInterval=5",
                    "-o", "ServerAliveCountMax=3",
                    host,
                    &ssh_cmd,
                ];

                // Try up to 3 times (initial + 2 retries) with exponential backoff
                let mut raw = String::new();
                let mut succeeded = false;
                for attempt in 0..3 {
                    if attempt > 0 {
                        let backoff = Duration::from_secs(1 << (attempt - 1)); // 1s, 2s
                        thread::sleep(backoff);
                        if shutdown_remote.load(Ordering::Relaxed) { break; }
                    }
                    raw = run_cmd("ssh", &ssh_args);
                    if !raw.is_empty() && raw.contains('{') {
                        succeeded = true;
                        break;
                    }
                }

                if succeeded {
                    if consecutive_failures > 0 {
                        log::info!("Remote SSH polling: '{}' recovered after {} failures", host, consecutive_failures);
                    }
                    consecutive_failures = 0;
                    let activity = parse_raw_activity(&raw);
                    let info = get_remote_agent_info(host, jsonl_dir, &raw);
                    if let Ok(mut cache) = remote_cache_writer.lock() {
                        cache.pixel_activity = activity;
                        cache.pixel_agent_info = info;
                    }
                } else {
                    consecutive_failures += 1;
                    // Log at specific thresholds to avoid log spam on long outages
                    if consecutive_failures == 1 || consecutive_failures == 5 || consecutive_failures % 20 == 0 {
                        log::warn!(
                            "Remote SSH polling: '{}' unreachable (consecutive failures: {}{})",
                            host, consecutive_failures,
                            if consecutive_failures >= 5 { ", polling at 30s" } else { "" }
                        );
                    }
                }
            }
        }
    });

    // Main monitor thread — local data + cached remote data (every 3s, never blocks on SSH)
    let remote_cache_reader = remote_cache.clone();
    let shutdown_main = shutdown.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(3));
        if shutdown_main.load(Ordering::Relaxed) { break; }
        let store = app_session.state::<TaskStore>();
        let registry = app_session.state::<MachineRegistry>();

        // Get ALL local machine JSONL dirs (lock + release before I/O)
        let local_jsonl_dirs: Vec<String> = {
            let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
            let local_machine = machines.values().find(|m| m.host == "local" && m.enabled);
            if let Some(m) = local_machine {
                if m.repos.is_empty() {
                    m.repo_path.iter().map(|p| repo_path_to_jsonl_dir(p, true, None)).collect()
                } else {
                    m.repos.iter().map(|r| repo_path_to_jsonl_dir(&r.path, true, None)).collect()
                }
            } else {
                vec![]
            }
        };

        let session = get_session_data_full(&store, Some(&registry));
        let _ = app_session.emit("session-update", &session);

        // Local activity from ALL repos (fast, no SSH)
        let mut atlas_activity: Vec<Activity> = Vec::new();
        let mut atlas_agent_info = AgentInfo { agent_count: 0, skills: vec![] };
        for dir in &local_jsonl_dirs {
            if let Some(jsonl_path) = get_latest_jsonl(dir) {
                let activity = parse_jsonl_activity(&jsonl_path, 150);
                if activity.len() > atlas_activity.len() {
                    atlas_activity = activity; // use the most active repo's feed
                }
            }
            let info = get_local_agent_info(dir);
            atlas_agent_info.agent_count += info.agent_count;
            for s in info.skills {
                if !atlas_agent_info.skills.contains(&s) {
                    atlas_agent_info.skills.push(s);
                }
            }
        }

        // Remote activity from cache (instant, no blocking)
        let (pixel_activity, pixel_agent_info) = {
            let cache = remote_cache_reader.lock().unwrap_or_else(|e| e.into_inner());
            (cache.pixel_activity.clone(), cache.pixel_agent_info.clone())
        };

        let _ = app_session.emit(
            "activity-update",
            serde_json::json!({
                "atlas": atlas_activity,
                "pixel": pixel_activity,
                "atlasAgentInfo": atlas_agent_info,
                "pixelAgentInfo": pixel_agent_info
            }),
        );
    });

    // Commits update every 60s
    let shutdown_commits = shutdown.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        if shutdown_commits.load(Ordering::Relaxed) { break; }
        let store = app_commits.state::<TaskStore>();
        let registry = app_commits.state::<MachineRegistry>();
        let session = get_session_data_full(&store, Some(&registry));
        let _ = app_commits.emit(
            "commits-update",
            serde_json::json!({
                "back": session.commits_back,
                "front": session.commits_front
            }),
        );
    });
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_session_data(
    store: tauri::State<'_, TaskStore>,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<SessionData, String> {
    // Extract data from State before entering spawn_blocking (State<'_> is not Send)
    let config_snapshot = store.config.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let machines_snapshot: Vec<crate::types::Machine> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.values().cloned().collect()
    };

    tauri::async_runtime::spawn_blocking(move || {
        // Rebuild a temporary registry from the cloned machine snapshot
        let machines_map: std::collections::HashMap<String, crate::types::Machine> =
            machines_snapshot.into_iter().map(|m| (m.id.clone(), m)).collect();
        let temp_registry = MachineRegistry::from_machines(machines_map);
        get_session_data_with_config(config_snapshot, Some(&temp_registry))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_atlas_activity(registry: tauri::State<'_, MachineRegistry>) -> Vec<Activity> {
    let jsonl_dirs: Vec<String> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let local_machine = machines.values().find(|m| m.host == "local" && m.enabled);
        if let Some(m) = local_machine {
            if m.repos.is_empty() {
                m.repo_path.iter().map(|p| repo_path_to_jsonl_dir(p, true, None)).collect()
            } else {
                m.repos.iter().map(|r| repo_path_to_jsonl_dir(&r.path, true, None)).collect()
            }
        } else {
            vec![]
        }
    };
    // Return the most active repo's feed
    let mut best: Vec<Activity> = vec![];
    for dir in &jsonl_dirs {
        let activity = get_latest_jsonl(dir)
            .map(|j| parse_jsonl_activity(&j, 150))
            .unwrap_or_default();
        if activity.len() > best.len() {
            best = activity;
        }
    }
    best
}

#[tauri::command]
pub async fn get_pixel_activity_cmd(registry: tauri::State<'_, MachineRegistry>) -> Result<Vec<Activity>, String> {
    // Extract data from State before spawn_blocking (State<'_> is not Send)
    let info: Option<(String, String)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let remote = machines.values().find(|m| m.host != "local" && m.enabled);
        remote.map(|m| {
            let repo = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
            (m.host.clone(), repo_path_to_jsonl_dir(&repo, false, m.home_dir.as_deref()))
        })
    };
    match info {
        None => Ok(vec![]),
        Some((host, dir)) => {
            tauri::async_runtime::spawn_blocking(move || {
                get_remote_activity(&host, &dir, 80)
            })
            .await
            .map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
pub fn get_agent_details(target: String, registry: tauri::State<'_, MachineRegistry>) -> Vec<AgentDetail> {
    if target.is_empty() {
        return vec![];
    }
    let info = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let machine = machines.get(&target);
        machine.map(|m| {
            let repo = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
            let is_local = m.host == "local";
            (m.host.clone(), repo_path_to_jsonl_dir(&repo, is_local, m.home_dir.as_deref()), is_local)
        })
    };

    match info {
        Some((_, dir, true)) => get_local_agent_details(&dir),
        Some((host, dir, false)) => get_remote_agent_details(&host, &dir),
        None => vec![],
    }
}

#[tauri::command]
pub async fn get_agent_log(
    target: String,
    offset: usize,
    limit: usize,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<(Vec<LogEntry>, usize), String> {
    if target.is_empty() {
        return Ok((vec![], 0));
    }
    // Cap limit to avoid allocating unbounded memory from a frontend request
    let limit = limit.min(5000);
    // Extract data from State before spawn_blocking (State<'_> is not Send)
    let info: Option<(String, String, bool)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let machine = machines.get(&target);
        machine.map(|m| {
            let repo = m
                .repos
                .first()
                .map(|r| r.path.clone())
                .or(m.repo_path.clone())
                .unwrap_or_default();
            let is_local = m.host == "local";
            (
                m.host.clone(),
                repo_path_to_jsonl_dir(&repo, is_local, m.home_dir.as_deref()),
                is_local,
            )
        })
    };

    match info {
        Some((_, dir, true)) => {
            // Local: read latest JSONL file directly (fast, no blocking SSH)
            tauri::async_runtime::spawn_blocking(move || {
                if let Some(jsonl_path) = get_latest_jsonl(&dir) {
                    let raw = fs::read_to_string(&jsonl_path).unwrap_or_default();
                    parse_jsonl_log(&raw, offset, limit)
                } else {
                    (vec![], 0)
                }
            })
            .await
            .map_err(|e| e.to_string())
        }
        Some((host, jsonl_dir, false)) => {
            // Remote: SSH cat the latest JSONL — blocking, must use spawn_blocking
            tauri::async_runtime::spawn_blocking(move || {
                let ssh_cmd = format!(
                    "JSONL=$(ls -t {}*.jsonl 2>/dev/null | head -1); cat \"$JSONL\" 2>/dev/null",
                    shell_escape(&jsonl_dir)
                );
                let raw = run_cmd(
                    "ssh",
                    &[
                        "-o", "ConnectTimeout=3",
                        "-o", "ServerAliveInterval=5",
                        "-o", "ServerAliveCountMax=3",
                        &host,
                        &ssh_cmd,
                    ],
                );
                parse_jsonl_log(&raw, offset, limit)
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok((vec![], 0)),
    }
}

// ---------------------------------------------------------------------------
// New Analytics Commands
// ---------------------------------------------------------------------------

/// Returns a high-level summary of the current session state by scanning all
/// local JSONL files. Fields:
///   - active_machines: machine IDs that have a JSONL file modified in the last 5 minutes
///   - total_tokens_today: sum of `usage.output_tokens` from JSONL entries written today
///   - active_session_duration_secs: elapsed seconds since the earliest JSONL entry today
///   - last_activity_ts: ISO-8601 timestamp of the most recent JSONL entry across all repos
#[tauri::command]
pub fn get_session_summary(registry: tauri::State<'_, MachineRegistry>) -> serde_json::Value {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect (machine_id, jsonl_dir, is_local) for every enabled local machine+repo
    let local_dirs: Vec<(String, String)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for m in machines.values().filter(|m| m.host == "local" && m.enabled) {
            if m.repos.is_empty() {
                if let Some(p) = &m.repo_path {
                    out.push((m.id.clone(), repo_path_to_jsonl_dir(p, true, None)));
                }
            } else {
                for r in &m.repos {
                    out.push((m.id.clone(), repo_path_to_jsonl_dir(&r.path, true, None)));
                }
            }
        }
        out
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Today's midnight in UTC seconds
    let today_midnight = now_secs - (now_secs % 86400);

    let mut active_machines: Vec<String> = Vec::new();
    let mut total_tokens_today: u64 = 0;
    let mut earliest_ts_today: Option<u64> = None;
    let mut latest_ts: Option<u64> = None;
    let mut latest_ts_str: Option<String> = None;

    for (machine_id, dir) in &local_dirs {
        // Find latest JSONL in this dir
        let jsonl_path = match get_latest_jsonl(dir) {
            Some(p) => p,
            None => continue,
        };

        // Check if file was modified in the last 5 minutes
        let modified_recently = fs::metadata(&jsonl_path)
            .and_then(|m| m.modified())
            .ok()
            .map(|mtime| {
                mtime
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    + 300
                    > now_secs
            })
            .unwrap_or(false);

        if modified_recently && !active_machines.contains(machine_id) {
            active_machines.push(machine_id.clone());
        }

        // Parse JSONL for token counts and timestamps
        let raw = fs::read_to_string(&jsonl_path).unwrap_or_default();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Parse timestamp — Claude JSONL entries carry "timestamp" (ISO-8601)
            let ts_secs: Option<u64> = v["timestamp"].as_str().and_then(|ts| {
                // Minimal ISO-8601 parser: try to get a UNIX epoch via std
                // Format: "2024-11-15T14:30:00.000Z" or "2024-11-15T14:30:00Z"
                parse_iso8601_to_secs(ts)
            });

            if let Some(ts) = ts_secs {
                if latest_ts.map_or(true, |l| ts > l) {
                    latest_ts = Some(ts);
                    latest_ts_str = v["timestamp"].as_str().map(|s| s.to_string());
                }
                if ts >= today_midnight {
                    if earliest_ts_today.map_or(true, |e| ts < e) {
                        earliest_ts_today = Some(ts);
                    }
                    // Sum output tokens for today
                    if let Some(tokens) = v["usage"]["output_tokens"].as_u64() {
                        total_tokens_today += tokens;
                    }
                    // Also check message.usage
                    if let Some(tokens) = v["message"]["usage"]["output_tokens"].as_u64() {
                        total_tokens_today += tokens;
                    }
                }
            }
        }
    }

    let active_session_duration_secs = earliest_ts_today
        .map(|e| now_secs.saturating_sub(e))
        .unwrap_or(0);

    serde_json::json!({
        "active_machines": active_machines,
        "total_tokens_today": total_tokens_today,
        "active_session_duration_secs": active_session_duration_secs,
        "last_activity_ts": latest_ts_str,
    })
}

/// Scan all local JSONL files and return the last `limit` human-turn prompts
/// (role == "human" or role == "user" with type == "text") sent to agents.
#[tauri::command]
pub fn get_recent_prompts(
    limit: usize,
    registry: tauri::State<'_, MachineRegistry>,
) -> Vec<String> {
    let limit = limit.clamp(1, 200);

    let local_dirs: Vec<String> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for m in machines.values().filter(|m| m.host == "local" && m.enabled) {
            if m.repos.is_empty() {
                if let Some(p) = &m.repo_path {
                    out.push(repo_path_to_jsonl_dir(p, true, None));
                }
            } else {
                for r in &m.repos {
                    out.push(repo_path_to_jsonl_dir(&r.path, true, None));
                }
            }
        }
        out
    };

    // Collect (timestamp_secs, text) pairs so we can sort across repos
    let mut all: Vec<(u64, String)> = Vec::new();

    for dir in &local_dirs {
        // Look at all JSONL files in this dir, not just the latest
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let jsonl_files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
            .map(|e| e.path().to_string_lossy().to_string())
            .collect();

        for path in &jsonl_files {
            let raw = fs::read_to_string(path).unwrap_or_default();
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let msg = &v["message"];
                let role = msg["role"].as_str().unwrap_or("");
                if role != "human" && role != "user" {
                    continue;
                }

                let ts = v["timestamp"]
                    .as_str()
                    .and_then(parse_iso8601_to_secs)
                    .unwrap_or(0);

                let content = msg["content"].as_array();
                if let Some(blocks) = content {
                    for block in blocks {
                        if block["type"].as_str() == Some("text") {
                            let text = block["text"].as_str().unwrap_or("").trim();
                            if !text.is_empty() {
                                all.push((ts, text.chars().take(500).collect()));
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by timestamp descending, return the most recent `limit` prompts
    all.sort_by(|a, b| b.0.cmp(&a.0));
    all.dedup_by(|a, b| a.1 == b.1); // deduplicate identical prompts
    all.into_iter().take(limit).map(|(_, text)| text).collect()
}

/// Returns an activity heatmap for the last `days` days.
/// Each entry: {hour: 0–23, day_of_week: 0–6 (Mon=0), count: u32}
#[tauri::command]
pub fn get_activity_heatmap(
    days: u32,
    registry: tauri::State<'_, MachineRegistry>,
) -> Vec<serde_json::Value> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let days = days.clamp(1, 365);

    let local_dirs: Vec<String> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for m in machines.values().filter(|m| m.host == "local" && m.enabled) {
            if m.repos.is_empty() {
                if let Some(p) = &m.repo_path {
                    out.push(repo_path_to_jsonl_dir(p, true, None));
                }
            } else {
                for r in &m.repos {
                    out.push(repo_path_to_jsonl_dir(&r.path, true, None));
                }
            }
        }
        out
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now_secs.saturating_sub(u64::from(days) * 86400);

    // heatmap[day_of_week][hour] = count
    let mut heatmap = [[0u32; 24]; 7];

    for dir in &local_dirs {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let jsonl_files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
            .map(|e| e.path().to_string_lossy().to_string())
            .collect();

        for path in &jsonl_files {
            // Skip files that haven't been touched in the window
            let mtime = fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            if mtime < cutoff {
                continue;
            }

            let raw = fs::read_to_string(path).unwrap_or_default();
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let ts = match v["timestamp"].as_str().and_then(parse_iso8601_to_secs) {
                    Some(t) => t,
                    None => continue,
                };

                if ts < cutoff {
                    continue;
                }

                let hour = ((ts % 86400) / 3600) as usize;
                // day_of_week: days since Unix epoch; epoch was Thursday (3).
                // We want Mon=0 … Sun=6.
                let days_since_epoch = ts / 86400;
                let dow = ((days_since_epoch + 3) % 7) as usize; // Thu epoch + 3 → Mon=0

                heatmap[dow][hour] += 1;
            }
        }
    }

    // Flatten into a Vec<Value>
    let mut result = Vec::with_capacity(7 * 24);
    for (dow, day_row) in heatmap.iter().enumerate() {
        for (hour, count) in day_row.iter().enumerate() {
            result.push(serde_json::json!({
                "hour": hour,
                "day_of_week": dow,
                "count": count,
            }));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Timestamp helper
// ---------------------------------------------------------------------------

/// Parse a subset of ISO-8601 timestamps to Unix seconds.
/// Handles "2024-11-15T14:30:00Z", "2024-11-15T14:30:00.123Z", and
/// "2024-11-15T14:30:00+00:00" variants without external crates.
pub fn parse_iso8601_to_secs(ts: &str) -> Option<u64> {
    // Expect at least "YYYY-MM-DDTHH:MM:SS"
    if ts.len() < 19 {
        return None;
    }
    let year: u64 = ts[0..4].parse().ok()?;
    let month: u64 = ts[5..7].parse().ok()?;
    let day: u64 = ts[8..10].parse().ok()?;
    let hour: u64 = ts[11..13].parse().ok()?;
    let minute: u64 = ts[14..16].parse().ok()?;
    let second: u64 = ts[17..19].parse().ok()?;

    // Validate ranges to avoid panics
    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    // Days since Unix epoch (1970-01-01) — simplified Gregorian
    let y = year;
    let m = month;
    let d = day;

    // Number of days from year 0 to start of year y
    let y0 = y - 1;
    let days_to_year = 365 * y0 + y0 / 4 - y0 / 100 + y0 / 400;

    // Days in each month (non-leap)
    let month_days: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut days_in_months: u64 = 0;
    for (i, &days) in month_days.iter().enumerate().take(m as usize - 1) {
        days_in_months += days;
        if i == 1 && is_leap {
            days_in_months += 1;
        }
    }

    // Days from epoch (1970-01-01 = day 719_163 from year 0)
    let days_from_epoch = days_to_year + days_in_months + d - 1 - 719_162;

    Some(days_from_epoch * 86400 + hour * 3600 + minute * 60 + second)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // extract_match — regex helper
    // -------------------------------------------------------------------------

    #[test]
    fn extract_match_finds_objetivo() {
        let text = "Sesion iniciada\nObjetivo: Build the API\nRama: feature/api";
        let result = extract_match(text, r"Objetivo: (.+)");
        assert_eq!(result.as_deref(), Some("Build the API"));
    }

    #[test]
    fn extract_match_finds_rama() {
        let text = "Sesion iniciada\nObjetivo: Build the API\nRama: feature/api\nRondas: 3";
        let result = extract_match(text, r"Rama: (\S+)");
        assert_eq!(result.as_deref(), Some("feature/api"));
    }

    #[test]
    fn extract_match_finds_rondas_digit() {
        let text = "Rondas: 5\n";
        let result = extract_match(text, r"Rondas: (\d+)");
        assert_eq!(result.as_deref(), Some("5"));
    }

    #[test]
    fn extract_match_returns_none_when_no_match() {
        let text = "Nothing interesting here";
        let result = extract_match(text, r"Objetivo: (.+)");
        assert!(result.is_none());
    }

    #[test]
    fn extract_match_returns_none_on_empty_input() {
        let result = extract_match("", r"Objetivo: (.+)");
        assert!(result.is_none());
    }

    #[test]
    fn extract_match_returns_none_for_invalid_regex() {
        // Invalid regex should not panic — returns None
        let result = extract_match("hello", r"[invalid(regex");
        assert!(result.is_none());
    }

    #[test]
    fn extract_match_captures_first_group_only() {
        // Pattern has two groups; only group 1 is returned
        let text = "Key: value extra";
        let result = extract_match(text, r"Key: (\S+)");
        assert_eq!(result.as_deref(), Some("value"));
    }

    #[test]
    fn extract_match_multiline_picks_first_occurrence() {
        let text = "Objetivo: First\nObjetivo: Second\n";
        let result = extract_match(text, r"Objetivo: (.+)");
        // regex captures the first match
        assert_eq!(result.as_deref(), Some("First"));
    }

    // -------------------------------------------------------------------------
    // atlas_running detection logic (inline in get_session_data_full)
    //
    // The detection is: ps line contains " claude -p " or " claude --print "
    // (and does NOT contain "grep" or "ssh").
    // We extract the predicate into a helper closure and test its semantics.
    // -------------------------------------------------------------------------

    /// Mirrors the atlas_running line predicate from get_session_data_full.
    fn is_atlas_ps_line(line: &str) -> bool {
        !line.contains("grep") && !line.contains("ssh") &&
        (line.contains(" claude -p ") || line.contains(" claude --print ") || line.ends_with(" claude --print"))
    }

    /// Mirrors the pixel_running line predicate from get_session_data_full.
    fn is_pixel_ps_line(line: &str, remote_host: &str) -> bool {
        !line.contains("grep") &&
        line.contains("ssh") && line.contains(remote_host) &&
        (line.contains("claude -p") || line.contains("claude --print"))
    }

    #[test]
    fn atlas_detection_matches_claude_p_space() {
        let line = "ema  1234  claude -p some-prompt-here";
        assert!(is_atlas_ps_line(line));
    }

    #[test]
    fn atlas_detection_matches_claude_print_flag() {
        let line = "ema  1234  claude --print some-prompt";
        assert!(is_atlas_ps_line(line));
    }

    #[test]
    fn atlas_detection_matches_claude_print_at_end_of_line() {
        // ends_with(" claude --print") requires a space immediately before "claude"
        // Typical ps aux output when the binary is invoked as bare "claude":
        let line = "ema  5678  0.0  0.1  ...  claude --print";
        assert!(is_atlas_ps_line(line), "bare 'claude --print' at end-of-line should match");
        // " claude --print " with args after also matches via contains(" claude --print ")
        let line2 = "ema  5679  0.0  0.1  ...  claude --print session123";
        assert!(is_atlas_ps_line(line2), "' claude --print ' with args after should match");
    }

    #[test]
    fn atlas_detection_rejects_grep_lines() {
        // grep process itself should not be counted as running claude
        let line = "ema  9999  grep claude -p";
        assert!(!is_atlas_ps_line(line));
    }

    #[test]
    fn atlas_detection_rejects_ssh_lines() {
        // An SSH line (which is how pixel runs) must not count as atlas
        let line = "ema  9999  ssh pixel claude -p task";
        assert!(!is_atlas_ps_line(line));
    }

    #[test]
    fn atlas_detection_rejects_unrelated_process() {
        let line = "ema  1111  python3 myapp.py";
        assert!(!is_atlas_ps_line(line));
    }

    #[test]
    fn atlas_detection_rejects_interactive_claude_no_p_flag() {
        // An interactive `claude` session (no -p / --print) should NOT match
        let line = "ema  2222  claude";
        assert!(!is_atlas_ps_line(line));
    }

    #[test]
    fn pixel_detection_matches_ssh_to_remote_host_with_claude_p() {
        let line = "ema  3333  ssh pixel claude -p do something";
        assert!(is_pixel_ps_line(line, "pixel"));
    }

    #[test]
    fn pixel_detection_matches_ssh_with_claude_print() {
        let line = "ema  4444  ssh mybox claude --print the task";
        assert!(is_pixel_ps_line(line, "mybox"));
    }

    #[test]
    fn pixel_detection_rejects_grep_lines() {
        let line = "ema  9999  grep ssh pixel claude -p";
        assert!(!is_pixel_ps_line(line, "pixel"));
    }

    #[test]
    fn pixel_detection_rejects_ssh_to_different_host() {
        // SSH to "atlas" — shouldn't match when looking for "pixel"
        let line = "ema  5555  ssh atlas claude -p task";
        assert!(!is_pixel_ps_line(line, "pixel"));
    }

    #[test]
    fn pixel_detection_rejects_local_claude_process() {
        // A local claude -p line (no ssh) should not match pixel
        let line = "ema  6666  claude -p local task";
        assert!(!is_pixel_ps_line(line, "pixel"));
    }

    #[test]
    fn pixel_detection_requires_ssh_keyword() {
        // Contains "pixel" and "claude -p" but no "ssh" → should not match
        let line = "ema  7777  /usr/bin/pixel-runner claude -p task";
        assert!(!is_pixel_ps_line(line, "pixel"));
    }

    // -------------------------------------------------------------------------
    // parse_iso8601_to_secs
    // -------------------------------------------------------------------------

    #[test]
    fn parse_iso8601_epoch() {
        // Unix epoch itself
        let secs = super::parse_iso8601_to_secs("1970-01-01T00:00:00Z");
        assert_eq!(secs, Some(0));
    }

    #[test]
    fn parse_iso8601_known_timestamp() {
        // 2024-01-01T00:00:00Z → verify deterministically
        let secs = super::parse_iso8601_to_secs("2024-01-01T00:00:00Z").unwrap_or(0);
        // 2024-01-01 is 54 years + leap days after 1970
        // Expected: 1704067200  (verified with `date -d "2024-01-01" +%s` on Linux)
        assert_eq!(secs, 1_704_067_200);
    }

    #[test]
    fn parse_iso8601_with_millis() {
        // Trailing fractional seconds and timezone should not break parsing
        let secs = super::parse_iso8601_to_secs("2024-01-01T00:00:00.123Z");
        assert_eq!(secs, Some(1_704_067_200));
    }

    #[test]
    fn parse_iso8601_returns_none_on_invalid() {
        assert!(super::parse_iso8601_to_secs("not-a-date").is_none());
        assert!(super::parse_iso8601_to_secs("").is_none());
        assert!(super::parse_iso8601_to_secs("2024-99-99T00:00:00Z").is_none());
    }

    #[test]
    fn parse_iso8601_leap_year_feb29() {
        // 2024 is a leap year; 2024-02-29T00:00:00Z should parse correctly
        // Days since epoch for 2024-02-29:
        //   2024-01-01 = 1704067200, + 31 (Jan) + 28 (Feb 1..28) = 59 days later
        let secs = super::parse_iso8601_to_secs("2024-02-29T00:00:00Z").unwrap_or(0);
        let jan1_2024: u64 = 1_704_067_200;
        let expected = jan1_2024 + 59 * 86400;
        assert_eq!(secs, expected);
    }

    // -------------------------------------------------------------------------
    // Activity heatmap aggregation logic (pure, no filesystem)
    // -------------------------------------------------------------------------

    /// Build a minimal heatmap from a slice of (ts_secs, count) pairs,
    /// mirroring the logic used in get_activity_heatmap.
    fn build_heatmap_from_timestamps(timestamps: &[u64]) -> [[u32; 24]; 7] {
        let mut heatmap = [[0u32; 24]; 7];
        for &ts in timestamps {
            let hour = ((ts % 86400) / 3600) as usize;
            let dow = ((ts / 86400 + 3) % 7) as usize;
            heatmap[dow][hour] += 1;
        }
        heatmap
    }

    #[test]
    fn heatmap_epoch_is_thursday_hour0() {
        // Unix epoch (ts=0) is 1970-01-01, a Thursday. Mon=0 … Thu=3.
        let heatmap = build_heatmap_from_timestamps(&[0]);
        assert_eq!(heatmap[3][0], 1, "epoch should land on Thursday (dow=3) hour 0");
    }

    #[test]
    fn heatmap_multiple_events_same_slot_accumulate() {
        // Three events all at epoch → heatmap[3][0] should be 3
        let heatmap = build_heatmap_from_timestamps(&[0, 0, 0]);
        assert_eq!(heatmap[3][0], 3);
    }

    #[test]
    fn heatmap_different_hours_separated() {
        // Two events 1 hour apart on the same day
        let heatmap = build_heatmap_from_timestamps(&[0, 3600]);
        assert_eq!(heatmap[3][0], 1);
        assert_eq!(heatmap[3][1], 1);
    }

    #[test]
    fn heatmap_week_boundary_wraps_correctly() {
        // Exactly 7 days after epoch is also a Thursday
        let one_week = 7 * 86400u64;
        let heatmap = build_heatmap_from_timestamps(&[one_week]);
        assert_eq!(heatmap[3][0], 1, "7 days after epoch is still Thursday");
    }
}
