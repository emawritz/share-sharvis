use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::process::Command;

use crate::machines::MachineRegistry;
use crate::jsonl::{get_latest_jsonl, repo_path_to_jsonl_dir};
use crate::types::{
    shell_escape, DailyStat, ErrorContext, FileChange, HeatmapEntry, TimelineError, TimelineEvent,
    TimelineResponse, TimelineSummary, TokenStats, ToolStat,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn looks_like_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("error")
        || text.contains("FAIL")
        || lower.contains("failed")
        || lower.contains("enoent")
        || lower.contains("eacces")
        || lower.contains("exit code ")
        || lower.contains("exited with")
        || lower.contains("non-zero")
        || lower.contains("panic")
        || lower.contains("traceback")
}

fn extract_file_path(input: &serde_json::Value) -> String {
    input["file_path"]
        .as_str()
        .or_else(|| input["path"].as_str())
        .or_else(|| input["filePath"].as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_detail(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Bash" => input["description"]
            .as_str()
            .or_else(|| input["command"].as_str())
            .unwrap_or("")
            .chars()
            .take(200)
            .collect(),
        "Read" | "Edit" | "Write" => extract_file_path(input),
        "Grep" => format!("\"{}\"", input["pattern"].as_str().unwrap_or("")),
        "Glob" => input["pattern"].as_str().unwrap_or("").to_string(),
        "Agent" => input["prompt"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect(),
        _ => serde_json::to_string(input)
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect(),
    }
}

fn flatten_content_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter(|b| b["type"].as_str() == Some("text"))
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

// ---------------------------------------------------------------------------
// 1. Full timeline parser
// ---------------------------------------------------------------------------

pub fn parse_timeline(data: &str) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg = &entry["message"];
        let role = msg["role"].as_str().unwrap_or("").to_string();
        let ts = entry["timestamp"]
            .as_str()
            .or_else(|| entry["ts"].as_str())
            .unwrap_or("")
            .to_string();

        let usage = &msg["usage"];
        let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);

        let content = &msg["content"];

        if !content.is_array() {
            let text = content.as_str().unwrap_or("");
            // Skip entirely empty text events — they add no signal to the timeline.
            if text.is_empty() {
                continue;
            }
            events.push(TimelineEvent {
                timestamp: ts,
                role,
                type_: "text".into(),
                tool_name: None,
                detail: text.chars().take(400).collect(),
                command: None,
                file_path: None,
                tool_use_id: None,
                input_tokens,
                output_tokens,
                is_error: looks_like_error(text),
            });
            continue;
        }

        if let Some(blocks) = content.as_array() {
            for block in blocks {
                let block_type = block["type"].as_str().unwrap_or("");
                match block_type {
                    "text" => {
                        let text = block["text"].as_str().unwrap_or("");
                        events.push(TimelineEvent {
                            timestamp: ts.clone(),
                            role: role.clone(),
                            type_: "text".into(),
                            tool_name: None,
                            detail: text.chars().take(400).collect(),
                            command: None,
                            file_path: None,
                            tool_use_id: None,
                            input_tokens,
                            output_tokens,
                            is_error: looks_like_error(text),
                        });
                    }
                    "tool_use" => {
                        let name = block["name"].as_str().unwrap_or("unknown");
                        let input = &block["input"];
                        let detail = extract_detail(name, input);
                        let command = if name == "Bash" {
                            Some(input["command"].as_str().unwrap_or("").to_string())
                        } else {
                            None
                        };
                        let file_path = {
                            let fp = extract_file_path(input);
                            if fp.is_empty() { None } else { Some(fp) }
                        };

                        events.push(TimelineEvent {
                            timestamp: ts.clone(),
                            role: role.clone(),
                            type_: "tool_use".into(),
                            tool_name: Some(name.to_string()),
                            detail,
                            command,
                            file_path,
                            tool_use_id: None,
                            input_tokens,
                            output_tokens,
                            is_error: false,
                        });
                    }
                    "tool_result" => {
                        let result_text = flatten_content_text(&block["content"]);
                        let has_error = looks_like_error(&result_text);
                        events.push(TimelineEvent {
                            timestamp: ts.clone(),
                            role: role.clone(),
                            type_: "tool_result".into(),
                            tool_name: None,
                            detail: result_text.chars().take(400).collect(),
                            command: None,
                            file_path: None,
                            tool_use_id: block["tool_use_id"]
                                .as_str()
                                .map(|s| s.to_string()),
                            input_tokens,
                            output_tokens,
                            is_error: has_error,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    // Sort by timestamp
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    events
}

// ---------------------------------------------------------------------------
// 2. Session summary
// ---------------------------------------------------------------------------

pub fn summarize_session(timeline: &[TimelineEvent]) -> TimelineSummary {
    if timeline.is_empty() {
        return TimelineSummary {
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_tokens: 0,
            tool_calls: HashMap::new(),
            duration: 0,
            duration_human: "0s".into(),
            error_count: 0,
            files_touched: Vec::new(),
            commands_run: Vec::new(),
        };
    }

    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut tool_counts: HashMap<String, u64> = HashMap::new();
    let mut error_count: u64 = 0;
    let mut files_set: HashSet<String> = HashSet::new();
    let mut commands = Vec::new();
    let mut seen_timestamps: HashSet<String> = HashSet::new();

    for ev in timeline {
        let token_key = format!("{}|{}", ev.timestamp, ev.role);
        if !seen_timestamps.contains(&token_key) {
            seen_timestamps.insert(token_key);
            total_in += ev.input_tokens;
            total_out += ev.output_tokens;
        }

        if ev.type_ == "tool_use" {
            if let Some(ref name) = ev.tool_name {
                *tool_counts.entry(name.clone()).or_insert(0) += 1;

                if matches!(name.as_str(), "Read" | "Edit" | "Write") {
                    if let Some(ref fp) = ev.file_path {
                        if !fp.is_empty() {
                            files_set.insert(fp.clone());
                        }
                    }
                }

                if name == "Bash" {
                    if let Some(ref cmd) = ev.command {
                        if !cmd.is_empty() {
                            commands.push(cmd.chars().take(200).collect::<String>());
                        }
                    }
                }
            }
        }

        if ev.is_error {
            error_count += 1;
        }
    }

    // Duration
    let timestamps: Vec<i64> = timeline
        .iter()
        .filter(|e| !e.timestamp.is_empty())
        .filter_map(|e| chrono::DateTime::parse_from_rfc3339(&e.timestamp).ok())
        .map(|dt| dt.timestamp_millis())
        .collect();

    let (duration, duration_human) = if timestamps.len() >= 2 {
        let min_t = *timestamps.iter().min().unwrap_or(&0);
        let max_t = *timestamps.iter().max().unwrap_or(&0);
        let dur = max_t.saturating_sub(min_t) as u64;
        let secs = dur / 1000;
        let human = if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        };
        (dur, human)
    } else {
        (0, "0s".into())
    };

    let mut files_touched: Vec<String> = files_set.into_iter().collect();
    files_touched.sort();

    TimelineSummary {
        total_input_tokens: total_in,
        total_output_tokens: total_out,
        total_tokens: total_in + total_out,
        tool_calls: tool_counts,
        duration,
        duration_human,
        error_count,
        files_touched,
        commands_run: commands,
    }
}

// ---------------------------------------------------------------------------
// 3. Error extractor
// ---------------------------------------------------------------------------

pub fn extract_errors(timeline: &[TimelineEvent]) -> Vec<TimelineError> {
    let mut errors = Vec::new();

    for (i, ev) in timeline.iter().enumerate() {
        if !ev.is_error {
            continue;
        }

        let before = if i > 0 { Some(&timeline[i - 1]) } else { None };
        let after = if i < timeline.len() - 1 {
            Some(&timeline[i + 1])
        } else {
            None
        };

        let mut tool = ev.tool_name.clone().unwrap_or_default();
        let mut command = ev.command.clone().unwrap_or_default();

        if ev.type_ == "tool_result" && tool.is_empty() {
            for j in (i.saturating_sub(5)..i).rev() {
                if timeline[j].type_ == "tool_use" {
                    tool = timeline[j].tool_name.clone().unwrap_or_default();
                    command = timeline[j]
                        .command
                        .clone()
                        .or_else(|| Some(timeline[j].detail.clone()))
                        .unwrap_or_default();
                    break;
                }
            }
        }

        errors.push(TimelineError {
            timestamp: ev.timestamp.clone(),
            tool: if tool.is_empty() {
                "unknown".into()
            } else {
                tool
            },
            command: command.chars().take(200).collect(),
            error: ev.detail.chars().take(500).collect(),
            context_before: before.map(|b| ErrorContext {
                type_: Some(b.type_.clone()),
                tool: b.tool_name.clone(),
                detail: Some(b.detail.chars().take(150).collect()),
            }),
            context_after: after.map(|a| ErrorContext {
                type_: Some(a.type_.clone()),
                tool: a.tool_name.clone(),
                detail: Some(a.detail.chars().take(150).collect()),
            }),
        });
    }

    errors
}

// ---------------------------------------------------------------------------
// 4. Activity heatmap
// ---------------------------------------------------------------------------

pub fn activity_heatmap(timeline: &[TimelineEvent]) -> Vec<HeatmapEntry> {
    let mut buckets: BTreeMap<String, (u64, HashMap<String, u64>)> = BTreeMap::new();

    for ev in timeline {
        if ev.timestamp.is_empty() {
            continue;
        }
        let dt = match chrono::DateTime::parse_from_rfc3339(&ev.timestamp) {
            Ok(dt) => dt,
            Err(_) => continue,
        };

        let key = format!("{}", dt.format("%H:%M"));

        let entry = buckets.entry(key).or_insert_with(|| (0, HashMap::new()));
        entry.0 += 1;

        if ev.type_ == "tool_use" {
            if let Some(ref name) = ev.tool_name {
                *entry.1.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    buckets
        .into_iter()
        .map(|(minute, (count, tools))| HeatmapEntry {
            minute,
            count,
            tools,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 5. File change tracker
// ---------------------------------------------------------------------------

pub fn file_changes(timeline: &[TimelineEvent]) -> Vec<FileChange> {
    let mut files: HashMap<String, (u64, u64, u64)> = HashMap::new();

    for ev in timeline {
        if ev.type_ != "tool_use" {
            continue;
        }
        let name = match &ev.tool_name {
            Some(n) => n.as_str(),
            None => continue,
        };
        if !matches!(name, "Read" | "Edit" | "Write") {
            continue;
        }
        let fp = match &ev.file_path {
            Some(fp) if !fp.is_empty() => fp.clone(),
            _ => continue,
        };

        let entry = files.entry(fp).or_insert((0, 0, 0));
        match name {
            "Read" => entry.0 += 1,
            "Edit" => entry.1 += 1,
            "Write" => entry.2 += 1,
            _ => {}
        }
    }

    let mut result: Vec<FileChange> = files
        .into_iter()
        .map(|(path, (reads, edits, writes))| FileChange {
            path,
            reads,
            edits,
            writes,
            total: reads + edits + writes,
        })
        .collect();

    result.sort_by(|a, b| b.total.cmp(&a.total));
    result
}

// ---------------------------------------------------------------------------
// 6. Remote timeline
// ---------------------------------------------------------------------------

pub fn get_remote_timeline(ssh_alias: &str, jsonl_pattern: &str) -> Vec<TimelineEvent> {
    if ssh_alias.is_empty() || jsonl_pattern.is_empty() {
        return Vec::new();
    }

    // Strip the trailing "*.jsonl" glob to get the directory prefix.
    // shell_escape is applied to the full glob so it is passed as a single
    // quoted argument; the glob characters must remain unquoted for the shell
    // to expand them, so we escape only the directory portion and concatenate
    // the literal glob suffix outside the quotes.
    let dir_prefix = jsonl_pattern.trim_end_matches("*.jsonl");
    // Build: ls -t '<escaped_dir>'*.jsonl 2>/dev/null | head -1
    // The single-quoted dir is immediately followed by *.jsonl (unquoted) so
    // the shell expands the glob.
    let find_cmd = format!(
        "ls -t {}*.jsonl 2>/dev/null | head -1",
        shell_escape(dir_prefix)
    );
    let latest_file = Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            ssh_alias,
            &find_cmd,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if latest_file.is_empty() {
        return Vec::new();
    }

    // Validate the returned path stays within the expected directory to prevent
    // a compromised remote from redirecting the cat to arbitrary files.
    // The remote always returns an absolute path, so resolve the expected prefix
    // to absolute form as well.  Paths starting with "~" are expanded by the
    // remote shell; we cannot know the remote home, so we accept any path that
    // ends with ".jsonl" and does not contain shell-hostile characters.
    let safe_path = latest_file.ends_with(".jsonl")
        && !latest_file.contains('\n')
        && !latest_file.contains('\r')
        && !latest_file.contains(';')
        && !latest_file.contains('&')
        && !latest_file.contains('|')
        && !latest_file.contains('`')
        && !latest_file.contains('$');

    if !safe_path {
        log::warn!("get_remote_timeline: rejected unsafe path from remote: {:?}", latest_file);
        return Vec::new();
    }

    let content = Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            ssh_alias,
            &format!("cat {}", shell_escape(&latest_file)),
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if content.is_empty() {
        return Vec::new();
    }

    parse_timeline(&content)
}

// ---------------------------------------------------------------------------
// Tauri Command
// ---------------------------------------------------------------------------


#[tauri::command]
pub async fn get_timeline(target: String, registry: tauri::State<'_, MachineRegistry>) -> Result<TimelineResponse, String> {
    // Extract machine info from State before spawn_blocking (State<'_> is not Send)
    let (is_local, host, jsonl_dir) = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        let machine = machines.get(&target);
        match machine {
            Some(m) => {
                let repo = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
                let local = m.host == "local";
                (local, m.host.clone(), repo_path_to_jsonl_dir(&repo, local, m.home_dir.as_deref()))
            }
            None => (false, target.clone(), String::new()),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        let timeline = if is_local {
            match get_latest_jsonl(&jsonl_dir) {
                Some(path) => {
                    let data = fs::read_to_string(&path).unwrap_or_default();
                    parse_timeline(&data)
                }
                None => Vec::new(),
            }
        } else {
            let pattern = format!("{}*.jsonl", jsonl_dir);
            get_remote_timeline(&host, &pattern)
        };

        let summary = summarize_session(&timeline);
        let errors = extract_errors(&timeline);
        let heatmap = activity_heatmap(&timeline);
        let files = file_changes(&timeline);
        let event_count = timeline.len();

        TimelineResponse {
            summary,
            errors: errors.into_iter().take(20).collect(),
            heatmap,
            files: files.into_iter().take(30).collect(),
            event_count,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Token / cost stats aggregator
// ---------------------------------------------------------------------------

/// Pricing constants (USD per token).
/// Approximation based on Claude Sonnet pricing.
/// We use model-specific pricing where the model name is known.
fn cost_for_model(model: &str, input: u64, output: u64) -> f64 {
    // Pricing: USD per million tokens
    let (price_in, price_out) = match model {
        m if m.contains("opus") => (15.0_f64, 75.0_f64),
        m if m.contains("sonnet") => (3.0_f64, 15.0_f64),
        m if m.contains("haiku") => (0.25_f64, 1.25_f64),
        _ => (3.0_f64, 15.0_f64), // default: Sonnet pricing
    };
    (input as f64 / 1_000_000.0) * price_in + (output as f64 / 1_000_000.0) * price_out
}

/// Parse token stats from a local JSONL directory.
/// Reads ALL .jsonl files (not just the latest) to aggregate today's data.
fn parse_token_stats_from_dir(dir: &str, today: &str) -> (u64, u64, u32, HashMap<String, (u64, u64)>) {
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut sessions_today: u32 = 0;
    // model -> (input_tokens, output_tokens)
    let mut by_model: HashMap<String, (u64, u64)> = HashMap::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (0, 0, 0, HashMap::new()),
    };

    let jsonl_files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    for file_path in jsonl_files {
        let data = match fs::read_to_string(&file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let mut file_has_today = false;
        // Dedup by (timestamp, role) same as summarize_session
        let mut seen: HashSet<String> = HashSet::new();

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let ts = entry["timestamp"]
                .as_str()
                .or_else(|| entry["ts"].as_str())
                .unwrap_or("");

            // Only count entries from today
            if !ts.starts_with(today) { continue; }
            file_has_today = true;

            let msg = &entry["message"];
            let role = msg["role"].as_str().unwrap_or("");
            let usage = &msg["usage"];
            let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
            let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);

            if input_tokens == 0 && output_tokens == 0 { continue; }

            // Dedup by timestamp+role (same approach as summarize_session)
            let key = format!("{}|{}", ts, role);
            if seen.contains(&key) { continue; }
            seen.insert(key);

            total_in += input_tokens;
            total_out += output_tokens;

            // Extract model from message
            let model = msg["model"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            let entry = by_model.entry(model).or_insert((0, 0));
            entry.0 += input_tokens;
            entry.1 += output_tokens;
        }

        if file_has_today {
            sessions_today += 1;
        }
    }

    (total_in, total_out, sessions_today, by_model)
}

#[tauri::command]
pub fn get_token_stats(registry: tauri::State<'_, MachineRegistry>) -> TokenStats {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Collect all local machine repos
    let machine_repos: Vec<(String, bool, Option<String>)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines
            .values()
            .filter(|m| m.enabled && m.host == "local")
            .flat_map(|m| {
                m.repos.iter().map(move |r| {
                    (r.path.clone(), m.host == "local", m.home_dir.clone())
                })
            })
            .collect()
    };

    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut sessions_today: u32 = 0;
    let mut by_model: HashMap<String, (u64, u64)> = HashMap::new();

    for (repo_path, is_local, home_dir) in &machine_repos {
        let dir = repo_path_to_jsonl_dir(repo_path, *is_local, home_dir.as_deref());
        let (t_in, t_out, sess, models) = parse_token_stats_from_dir(&dir, &today);
        total_in += t_in;
        total_out += t_out;
        sessions_today += sess;
        for (model, (m_in, m_out)) in models {
            let entry = by_model.entry(model).or_insert((0, 0));
            entry.0 += m_in;
            entry.1 += m_out;
        }
    }

    // Build cost_by_model map
    let cost_by_model: HashMap<String, f64> = by_model
        .iter()
        .map(|(model, (in_tok, out_tok))| {
            (model.clone(), cost_for_model(model, *in_tok, *out_tok))
        })
        .collect();

    let total_cost_usd: f64 = cost_by_model.values().sum();

    TokenStats {
        total_cost_usd,
        tokens_in: total_in,
        tokens_out: total_out,
        sessions_today,
        cost_by_model,
    }
}

// ---------------------------------------------------------------------------
// Daily stats aggregator
// ---------------------------------------------------------------------------

/// Parse token usage per day from a local JSONL directory.
/// Returns a map of date-string -> (total_tokens, cost_usd, event_count).
fn parse_daily_stats_from_dir(dir: &str) -> BTreeMap<String, (u64, f64, u64)> {
    let mut by_day: BTreeMap<String, (u64, f64, u64)> = BTreeMap::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return by_day,
    };

    let jsonl_files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    for file_path in jsonl_files {
        let data = match fs::read_to_string(&file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Dedup tokens by (timestamp, role) within each file
        let mut seen: HashSet<String> = HashSet::new();

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let ts = entry["timestamp"]
                .as_str()
                .or_else(|| entry["ts"].as_str())
                .unwrap_or("");
            if ts.is_empty() { continue; }

            // Date portion: first 10 chars of ISO 8601 timestamp (YYYY-MM-DD)
            let date: String = ts.chars().take(10).collect();

            let msg = &entry["message"];
            let role = msg["role"].as_str().unwrap_or("");
            let usage = &msg["usage"];
            let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
            let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);

            let total_tokens = input_tokens + output_tokens;

            // Count every event (tool_use, text, tool_result blocks) as +1
            let event_count: u64 = msg["content"]
                .as_array()
                .map(|arr| arr.len() as u64)
                .unwrap_or(1);

            let stat = by_day.entry(date).or_insert((0, 0.0, 0));
            stat.2 += event_count;

            if total_tokens == 0 { continue; }

            // Dedup tokens by timestamp+role
            let key = format!("{}|{}", ts, role);
            if seen.contains(&key) { continue; }
            seen.insert(key);

            let model = msg["model"].as_str().unwrap_or("unknown");
            let cost = cost_for_model(model, input_tokens, output_tokens);

            stat.0 += total_tokens;
            stat.1 += cost;
        }
    }

    by_day
}

#[tauri::command]
pub fn get_daily_stats(
    days: u32,
    registry: tauri::State<'_, MachineRegistry>,
) -> Vec<DailyStat> {
    let days = days.clamp(1, 365) as i64;
    let now = chrono::Utc::now();

    // Collect local machine repos
    let machine_repos: Vec<(String, bool, Option<String>)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines
            .values()
            .filter(|m| m.enabled && m.host == "local")
            .flat_map(|m| {
                m.repos.iter().map(move |r| {
                    (r.path.clone(), m.host == "local", m.home_dir.clone())
                })
            })
            .collect()
    };

    // Aggregate across all repos
    let mut combined: BTreeMap<String, (u64, f64, u64)> = BTreeMap::new();
    for (repo_path, is_local, home_dir) in &machine_repos {
        let dir = repo_path_to_jsonl_dir(repo_path, *is_local, home_dir.as_deref());
        for (date, (tokens, cost, events)) in parse_daily_stats_from_dir(&dir) {
            let entry = combined.entry(date).or_insert((0, 0.0, 0));
            entry.0 += tokens;
            entry.1 += cost;
            entry.2 += events;
        }
    }

    // Build result for the last `days` days, including days with zero activity
    let mut result: Vec<DailyStat> = (0..days)
        .map(|i| {
            let date = (now - chrono::Duration::days(i))
                .format("%Y-%m-%d")
                .to_string();
            let (tokens, cost_usd, events) = combined.get(&date).copied().unwrap_or((0, 0.0, 0));
            DailyStat { date, tokens, cost_usd, events }
        })
        .collect();

    // Return chronologically (oldest first)
    result.reverse();
    result
}

// ---------------------------------------------------------------------------
// Top-tools aggregator
// ---------------------------------------------------------------------------

/// Parse tool call counts from a local JSONL directory.
fn parse_tool_counts_from_dir(dir: &str) -> HashMap<String, u64> {
    let mut counts: HashMap<String, u64> = HashMap::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return counts,
    };

    let jsonl_files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    for file_path in jsonl_files {
        let data = match fs::read_to_string(&file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let content = &entry["message"]["content"];
            if let Some(blocks) = content.as_array() {
                for block in blocks {
                    if block["type"].as_str() == Some("tool_use") {
                        let name = block["name"].as_str().unwrap_or("unknown");
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    counts
}

#[tauri::command]
pub fn get_top_tools(
    limit: usize,
    registry: tauri::State<'_, MachineRegistry>,
) -> Vec<ToolStat> {
    let limit = limit.clamp(1, 100);

    let machine_repos: Vec<(String, bool, Option<String>)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines
            .values()
            .filter(|m| m.enabled && m.host == "local")
            .flat_map(|m| {
                m.repos.iter().map(move |r| {
                    (r.path.clone(), m.host == "local", m.home_dir.clone())
                })
            })
            .collect()
    };

    let mut combined: HashMap<String, u64> = HashMap::new();
    for (repo_path, is_local, home_dir) in &machine_repos {
        let dir = repo_path_to_jsonl_dir(repo_path, *is_local, home_dir.as_deref());
        for (name, count) in parse_tool_counts_from_dir(&dir) {
            *combined.entry(name).or_insert(0) += count;
        }
    }

    let mut stats: Vec<ToolStat> = combined
        .into_iter()
        .map(|(tool_name, calls)| ToolStat { tool_name, calls })
        .collect();

    // Sort descending by call count, then alphabetically for stable ordering
    stats.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.tool_name.cmp(&b.tool_name)));
    stats.truncate(limit);
    stats
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TimelineEvent;

    // Helper to build a minimal TimelineEvent for test use.
    #[allow(clippy::too_many_arguments)]
    fn make_event(
        ts: &str,
        role: &str,
        type_: &str,
        tool_name: Option<&str>,
        detail: &str,
        command: Option<&str>,
        file_path: Option<&str>,
        input_tokens: u64,
        output_tokens: u64,
        is_error: bool,
    ) -> TimelineEvent {
        TimelineEvent {
            timestamp: ts.to_string(),
            role: role.to_string(),
            type_: type_.to_string(),
            tool_name: tool_name.map(|s| s.to_string()),
            detail: detail.to_string(),
            command: command.map(|s| s.to_string()),
            file_path: file_path.map(|s| s.to_string()),
            tool_use_id: None,
            input_tokens,
            output_tokens,
            is_error,
        }
    }

    // -----------------------------------------------------------------------
    // looks_like_error
    // -----------------------------------------------------------------------

    #[test]
    fn looks_like_error_detects_lowercase_error() {
        assert!(looks_like_error("something error occurred"));
    }

    #[test]
    fn looks_like_error_detects_uppercase_fail() {
        assert!(looks_like_error("build FAIL: could not compile"));
    }

    #[test]
    fn looks_like_error_detects_failed() {
        assert!(looks_like_error("Process failed with code 1"));
    }

    #[test]
    fn looks_like_error_detects_enoent() {
        assert!(looks_like_error("ENOENT: no such file or directory"));
    }

    #[test]
    fn looks_like_error_detects_panic() {
        assert!(looks_like_error("thread 'main' panicked at src/main.rs:42"));
    }

    #[test]
    fn looks_like_error_detects_exit_code() {
        assert!(looks_like_error("exit code 127"));
    }

    #[test]
    fn looks_like_error_accepts_normal_text() {
        assert!(!looks_like_error("build succeeded in 3.2s"));
    }

    #[test]
    fn looks_like_error_accepts_empty_string() {
        assert!(!looks_like_error(""));
    }

    // -----------------------------------------------------------------------
    // extract_file_path
    // -----------------------------------------------------------------------

    #[test]
    fn extract_file_path_uses_file_path_key() {
        let v = serde_json::json!({"file_path": "/src/main.rs"});
        assert_eq!(extract_file_path(&v), "/src/main.rs");
    }

    #[test]
    fn extract_file_path_falls_back_to_path_key() {
        let v = serde_json::json!({"path": "/src/lib.rs"});
        assert_eq!(extract_file_path(&v), "/src/lib.rs");
    }

    #[test]
    fn extract_file_path_falls_back_to_file_path_camel() {
        let v = serde_json::json!({"filePath": "/src/foo.rs"});
        assert_eq!(extract_file_path(&v), "/src/foo.rs");
    }

    #[test]
    fn extract_file_path_returns_empty_when_no_key() {
        let v = serde_json::json!({"command": "ls"});
        assert_eq!(extract_file_path(&v), "");
    }

    // -----------------------------------------------------------------------
    // flatten_content_text
    // -----------------------------------------------------------------------

    #[test]
    fn flatten_content_text_returns_string_value_directly() {
        let v = serde_json::json!("hello world");
        assert_eq!(flatten_content_text(&v), "hello world");
    }

    #[test]
    fn flatten_content_text_joins_text_blocks() {
        let v = serde_json::json!([
            {"type": "text", "text": "first"},
            {"type": "image", "url": "http://example.com"},
            {"type": "text", "text": "second"}
        ]);
        let result = flatten_content_text(&v);
        assert_eq!(result, "first\nsecond");
    }

    #[test]
    fn flatten_content_text_returns_empty_for_non_string_non_array() {
        let v = serde_json::json!(42);
        assert_eq!(flatten_content_text(&v), "");
    }

    // -----------------------------------------------------------------------
    // extract_detail
    // -----------------------------------------------------------------------

    #[test]
    fn extract_detail_bash_prefers_description() {
        let input = serde_json::json!({"description": "list files", "command": "ls -la"});
        assert_eq!(extract_detail("Bash", &input), "list files");
    }

    #[test]
    fn extract_detail_bash_falls_back_to_command() {
        let input = serde_json::json!({"command": "cargo build"});
        assert_eq!(extract_detail("Bash", &input), "cargo build");
    }

    #[test]
    fn extract_detail_read_returns_file_path() {
        let input = serde_json::json!({"file_path": "/src/main.rs"});
        assert_eq!(extract_detail("Read", &input), "/src/main.rs");
    }

    #[test]
    fn extract_detail_grep_wraps_pattern_in_quotes() {
        let input = serde_json::json!({"pattern": "fn main"});
        assert_eq!(extract_detail("Grep", &input), "\"fn main\"");
    }

    #[test]
    fn extract_detail_glob_returns_pattern() {
        let input = serde_json::json!({"pattern": "**/*.rs"});
        assert_eq!(extract_detail("Glob", &input), "**/*.rs");
    }

    #[test]
    fn extract_detail_agent_truncates_to_120_chars() {
        let long_prompt: String = "x".repeat(200);
        let input = serde_json::json!({"prompt": long_prompt});
        let result = extract_detail("Agent", &input);
        assert_eq!(result.len(), 120);
    }

    // -----------------------------------------------------------------------
    // parse_timeline
    // -----------------------------------------------------------------------

    #[test]
    fn parse_timeline_empty_input_returns_empty() {
        assert!(parse_timeline("").is_empty());
    }

    #[test]
    fn parse_timeline_skips_invalid_json_lines() {
        let data = "not json\n{also invalid\n";
        assert!(parse_timeline(data).is_empty());
    }

    #[test]
    fn parse_timeline_parses_simple_text_message() {
        let line = r#"{"timestamp":"2025-01-01T10:00:00Z","message":{"role":"assistant","content":"Hello world","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let events = parse_timeline(line);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].role, "assistant");
        assert_eq!(events[0].type_, "text");
        assert_eq!(events[0].input_tokens, 100);
        assert_eq!(events[0].output_tokens, 50);
    }

    #[test]
    fn parse_timeline_skips_empty_text_content() {
        // content is an empty string — should be skipped
        let line = r#"{"timestamp":"2025-01-01T10:00:00Z","message":{"role":"assistant","content":"","usage":{"input_tokens":0,"output_tokens":0}}}"#;
        let events = parse_timeline(line);
        assert!(events.is_empty());
    }

    #[test]
    fn parse_timeline_parses_tool_use_block() {
        let line = r#"{"timestamp":"2025-01-01T10:00:00Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}],"usage":{"input_tokens":10,"output_tokens":5}}}"#;
        let events = parse_timeline(line);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, "tool_use");
        assert_eq!(events[0].tool_name.as_deref(), Some("Bash"));
        assert_eq!(events[0].command.as_deref(), Some("ls -la"));
    }

    #[test]
    fn parse_timeline_sorts_events_by_timestamp() {
        let line1 = r#"{"timestamp":"2025-01-01T10:05:00Z","message":{"role":"user","content":"second","usage":{}}}"#;
        let line2 = r#"{"timestamp":"2025-01-01T10:00:00Z","message":{"role":"user","content":"first","usage":{}}}"#;
        let data = format!("{}\n{}", line1, line2);
        let events = parse_timeline(&data);
        assert_eq!(events.len(), 2);
        assert!(events[0].timestamp < events[1].timestamp);
    }

    #[test]
    fn parse_timeline_marks_error_text() {
        let line = r#"{"timestamp":"2025-01-01T10:00:00Z","message":{"role":"tool","content":"ENOENT: no such file","usage":{}}}"#;
        let events = parse_timeline(line);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_error);
    }

    // -----------------------------------------------------------------------
    // summarize_session
    // -----------------------------------------------------------------------

    #[test]
    fn summarize_session_empty_timeline_returns_zeroes() {
        let summary = summarize_session(&[]);
        assert_eq!(summary.total_tokens, 0);
        assert_eq!(summary.error_count, 0);
        assert_eq!(summary.duration_human, "0s");
        assert!(summary.files_touched.is_empty());
    }

    #[test]
    fn summarize_session_counts_errors() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "text", None, "ok", None, None, 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "tool", "tool_result", None, "ENOENT", None, None, 0, 0, true),
            make_event("2025-01-01T10:02:00Z", "tool", "tool_result", None, "error occurred", None, None, 0, 0, true),
        ];
        let summary = summarize_session(&events);
        assert_eq!(summary.error_count, 2);
    }

    #[test]
    fn summarize_session_counts_tool_calls() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "ls", Some("ls"), None, 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "assistant", "tool_use", Some("Bash"), "pwd", Some("pwd"), None, 0, 0, false),
            make_event("2025-01-01T10:02:00Z", "assistant", "tool_use", Some("Read"), "/src/main.rs", None, Some("/src/main.rs"), 0, 0, false),
        ];
        let summary = summarize_session(&events);
        assert_eq!(summary.tool_calls.get("Bash").copied(), Some(2));
        assert_eq!(summary.tool_calls.get("Read").copied(), Some(1));
    }

    #[test]
    fn summarize_session_deduplicates_tokens_by_timestamp_role() {
        // Two events with same timestamp+role share a token count — only counted once
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "text", None, "hello", None, None, 100, 50, false),
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "ls", Some("ls"), None, 100, 50, false),
        ];
        let summary = summarize_session(&events);
        // Tokens counted only once because same ts+role
        assert_eq!(summary.total_input_tokens, 100);
        assert_eq!(summary.total_output_tokens, 50);
        assert_eq!(summary.total_tokens, 150);
    }

    #[test]
    fn summarize_session_collects_files_touched() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Edit"), "/a.rs", None, Some("/a.rs"), 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "assistant", "tool_use", Some("Write"), "/b.rs", None, Some("/b.rs"), 0, 0, false),
            make_event("2025-01-01T10:02:00Z", "assistant", "tool_use", Some("Read"), "/a.rs", None, Some("/a.rs"), 0, 0, false),
        ];
        let summary = summarize_session(&events);
        assert_eq!(summary.files_touched.len(), 2);
        assert!(summary.files_touched.contains(&"/a.rs".to_string()));
        assert!(summary.files_touched.contains(&"/b.rs".to_string()));
    }

    #[test]
    fn summarize_session_collects_bash_commands() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "cargo build", Some("cargo build"), None, 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "assistant", "tool_use", Some("Bash"), "cargo test", Some("cargo test"), None, 0, 0, false),
        ];
        let summary = summarize_session(&events);
        assert_eq!(summary.commands_run.len(), 2);
        assert!(summary.commands_run.contains(&"cargo build".to_string()));
        assert!(summary.commands_run.contains(&"cargo test".to_string()));
    }

    #[test]
    fn summarize_session_duration_human_uses_minutes_format() {
        // 90 seconds apart
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "user", "text", None, "start", None, None, 0, 0, false),
            make_event("2025-01-01T10:01:30Z", "user", "text", None, "end", None, None, 0, 0, false),
        ];
        let summary = summarize_session(&events);
        // 90s → "1m 30s"
        assert_eq!(summary.duration_human, "1m 30s");
    }

    #[test]
    fn summarize_session_duration_human_uses_hours_format() {
        let events = vec![
            make_event("2025-01-01T08:00:00Z", "user", "text", None, "start", None, None, 0, 0, false),
            make_event("2025-01-01T10:30:00Z", "user", "text", None, "end", None, None, 0, 0, false),
        ];
        let summary = summarize_session(&events);
        // 2.5 hours → "2h 30m"
        assert_eq!(summary.duration_human, "2h 30m");
    }

    // -----------------------------------------------------------------------
    // extract_errors
    // -----------------------------------------------------------------------

    #[test]
    fn extract_errors_empty_timeline_returns_empty() {
        assert!(extract_errors(&[]).is_empty());
    }

    #[test]
    fn extract_errors_skips_non_error_events() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "text", None, "all good", None, None, 0, 0, false),
        ];
        assert!(extract_errors(&events).is_empty());
    }

    #[test]
    fn extract_errors_captures_error_events() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "tool", "tool_result", None, "ENOENT: file not found", None, None, 0, 0, true),
        ];
        let errors = extract_errors(&events);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].error.contains("ENOENT"));
    }

    #[test]
    fn extract_errors_backfills_tool_from_preceding_tool_use() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "ls", Some("ls"), None, 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "tool", "tool_result", None, "exit code 127", None, None, 0, 0, true),
        ];
        let errors = extract_errors(&events);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].tool, "Bash");
    }

    // -----------------------------------------------------------------------
    // activity_heatmap
    // -----------------------------------------------------------------------

    #[test]
    fn activity_heatmap_empty_timeline_returns_empty() {
        assert!(activity_heatmap(&[]).is_empty());
    }

    #[test]
    fn activity_heatmap_groups_by_minute() {
        let events = vec![
            make_event("2025-01-01T10:30:00Z", "assistant", "text", None, "a", None, None, 0, 0, false),
            make_event("2025-01-01T10:30:15Z", "assistant", "text", None, "b", None, None, 0, 0, false),
            make_event("2025-01-01T11:00:00Z", "assistant", "text", None, "c", None, None, 0, 0, false),
        ];
        let heatmap = activity_heatmap(&events);
        // Should have two buckets: "10:30" and "11:00"
        assert_eq!(heatmap.len(), 2);
        let counts: std::collections::HashMap<_, _> = heatmap.iter().map(|e| (e.minute.clone(), e.count)).collect();
        assert_eq!(counts["10:30"], 2);
        assert_eq!(counts["11:00"], 1);
    }

    #[test]
    fn activity_heatmap_skips_events_with_empty_timestamp() {
        let events = vec![
            make_event("", "assistant", "text", None, "no ts", None, None, 0, 0, false),
        ];
        assert!(activity_heatmap(&events).is_empty());
    }

    #[test]
    fn activity_heatmap_counts_tool_use_by_name() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "ls", Some("ls"), None, 0, 0, false),
            make_event("2025-01-01T10:00:30Z", "assistant", "tool_use", Some("Read"), "/a.rs", None, Some("/a.rs"), 0, 0, false),
        ];
        let heatmap = activity_heatmap(&events);
        assert_eq!(heatmap.len(), 1);
        let entry = &heatmap[0];
        assert_eq!(entry.tools.get("Bash").copied(), Some(1));
        assert_eq!(entry.tools.get("Read").copied(), Some(1));
    }

    // -----------------------------------------------------------------------
    // file_changes
    // -----------------------------------------------------------------------

    #[test]
    fn file_changes_empty_returns_empty() {
        assert!(file_changes(&[]).is_empty());
    }

    #[test]
    fn file_changes_ignores_non_file_tool_events() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Bash"), "ls", Some("ls"), None, 0, 0, false),
        ];
        assert!(file_changes(&events).is_empty());
    }

    #[test]
    fn file_changes_counts_reads_edits_writes() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Read"), "/a.rs", None, Some("/a.rs"), 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "assistant", "tool_use", Some("Edit"), "/a.rs", None, Some("/a.rs"), 0, 0, false),
            make_event("2025-01-01T10:02:00Z", "assistant", "tool_use", Some("Write"), "/b.rs", None, Some("/b.rs"), 0, 0, false),
        ];
        let changes = file_changes(&events);
        let map: std::collections::HashMap<_, _> = changes.iter().map(|c| (c.path.clone(), c)).collect();
        let a = map["/a.rs"];
        assert_eq!(a.reads, 1);
        assert_eq!(a.edits, 1);
        assert_eq!(a.writes, 0);
        assert_eq!(a.total, 2);
        let b = map["/b.rs"];
        assert_eq!(b.writes, 1);
        assert_eq!(b.total, 1);
    }

    #[test]
    fn file_changes_sorts_by_total_descending() {
        let events = vec![
            make_event("2025-01-01T10:00:00Z", "assistant", "tool_use", Some("Read"), "/once.rs", None, Some("/once.rs"), 0, 0, false),
            make_event("2025-01-01T10:01:00Z", "assistant", "tool_use", Some("Read"), "/many.rs", None, Some("/many.rs"), 0, 0, false),
            make_event("2025-01-01T10:02:00Z", "assistant", "tool_use", Some("Edit"), "/many.rs", None, Some("/many.rs"), 0, 0, false),
            make_event("2025-01-01T10:03:00Z", "assistant", "tool_use", Some("Write"), "/many.rs", None, Some("/many.rs"), 0, 0, false),
        ];
        let changes = file_changes(&events);
        assert_eq!(changes[0].path, "/many.rs");
        assert_eq!(changes[1].path, "/once.rs");
    }

    // -----------------------------------------------------------------------
    // cost_for_model
    // -----------------------------------------------------------------------

    #[test]
    fn cost_for_model_opus_is_more_expensive_than_sonnet() {
        let opus = cost_for_model("claude-opus-4", 1_000_000, 1_000_000);
        let sonnet = cost_for_model("claude-sonnet-4", 1_000_000, 1_000_000);
        assert!(opus > sonnet);
    }

    #[test]
    fn cost_for_model_haiku_is_cheaper_than_sonnet() {
        let haiku = cost_for_model("claude-haiku-3", 1_000_000, 1_000_000);
        let sonnet = cost_for_model("claude-sonnet-4", 1_000_000, 1_000_000);
        assert!(haiku < sonnet);
    }

    #[test]
    fn cost_for_model_zero_tokens_is_zero_cost() {
        assert_eq!(cost_for_model("claude-sonnet-4", 0, 0), 0.0);
    }

    #[test]
    fn cost_for_model_unknown_model_uses_sonnet_pricing() {
        let unknown = cost_for_model("unknown-model-xyz", 1_000_000, 1_000_000);
        let sonnet = cost_for_model("claude-sonnet-4", 1_000_000, 1_000_000);
        assert!((unknown - sonnet).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // parse_daily_stats_from_dir
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn write_tmp_jsonl(name: &str, content: &str) -> String {
        let path = std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn make_usage_line(date: &str, role: &str, model: &str, input: u64, output: u64, tool_count: usize) -> String {
        let blocks: String = (0..tool_count)
            .map(|i| format!(r#"{{"type":"tool_use","name":"Bash","input":{{"command":"ls {}"}}}}"#, i))
            .collect::<Vec<_>>()
            .join(",");
        let content = if tool_count > 0 {
            format!("[{}]", blocks)
        } else {
            r#""hello""#.to_string()
        };
        format!(
            r#"{{"timestamp":"{}T10:00:00Z","message":{{"role":"{}","model":"{}","content":{},"usage":{{"input_tokens":{},"output_tokens":{}}}}}}}"#,
            date, role, model, content, input, output
        )
    }

    #[test]
    fn parse_daily_stats_from_dir_nonexistent_dir_returns_empty() {
        let result = parse_daily_stats_from_dir("/nonexistent/dir/");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_daily_stats_from_dir_aggregates_by_date() {
        let dir = std::env::temp_dir()
            .join("jarvis_daily_test")
            .to_string_lossy()
            .to_string();
        let _ = std::fs::create_dir_all(&dir);
        let file = format!("{}/session.jsonl", dir);
        let line1 = make_usage_line("2026-03-10", "assistant", "claude-sonnet-4", 1000, 500, 2);
        let line2 = make_usage_line("2026-03-11", "assistant", "claude-sonnet-4", 2000, 1000, 3);
        let line3 = make_usage_line("2026-03-11", "user", "claude-sonnet-4", 0, 0, 0);
        std::fs::write(&file, format!("{}\n{}\n{}\n", line1, line2, line3)).unwrap();

        let result = parse_daily_stats_from_dir(&format!("{}/", dir));
        assert!(result.contains_key("2026-03-10"), "must have 2026-03-10");
        assert!(result.contains_key("2026-03-11"), "must have 2026-03-11");
        // 2026-03-10: 1500 tokens
        assert_eq!(result["2026-03-10"].0, 1500);
        // 2026-03-11: 3000 tokens
        assert_eq!(result["2026-03-11"].0, 3000);
        // Cleanup
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn parse_daily_stats_from_dir_deduplicates_tokens_by_ts_role() {
        let dir = std::env::temp_dir()
            .join("jarvis_daily_dedup_test")
            .to_string_lossy()
            .to_string();
        let _ = std::fs::create_dir_all(&dir);
        let file = format!("{}/session.jsonl", dir);
        // Two lines with same timestamp+role → tokens counted only once
        let line1 = make_usage_line("2026-03-15", "assistant", "claude-sonnet-4", 500, 200, 1);
        let line2 = make_usage_line("2026-03-15", "assistant", "claude-sonnet-4", 500, 200, 1);
        std::fs::write(&file, format!("{}\n{}\n", line1, line2)).unwrap();

        let result = parse_daily_stats_from_dir(&format!("{}/", dir));
        assert_eq!(result["2026-03-15"].0, 700, "tokens counted only once for same ts+role");
        let _ = std::fs::remove_file(&file);
    }

    // -----------------------------------------------------------------------
    // parse_tool_counts_from_dir
    // -----------------------------------------------------------------------

    #[test]
    fn parse_tool_counts_from_dir_nonexistent_dir_returns_empty() {
        let result = parse_tool_counts_from_dir("/nonexistent/dir/");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_tool_counts_from_dir_counts_tool_uses() {
        let dir = std::env::temp_dir()
            .join("jarvis_tool_test")
            .to_string_lossy()
            .to_string();
        let _ = std::fs::create_dir_all(&dir);
        let file = format!("{}/session.jsonl", dir);
        let line1 = r#"{"message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}},{"type":"tool_use","name":"Read","input":{}}]}}"#;
        let line2 = r#"{"message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#;
        std::fs::write(&file, format!("{}\n{}\n", line1, line2)).unwrap();

        let result = parse_tool_counts_from_dir(&format!("{}/", dir));
        assert_eq!(result.get("Bash").copied(), Some(2));
        assert_eq!(result.get("Read").copied(), Some(1));
        let _ = std::fs::remove_file(&file);
    }

    // -----------------------------------------------------------------------
    // get_top_tools / get_daily_stats (pure helpers)
    // -----------------------------------------------------------------------

    #[test]
    fn tool_stat_sort_descending_by_calls() {
        let mut stats = [
            ToolStat { tool_name: "Read".into(), calls: 5 },
            ToolStat { tool_name: "Bash".into(), calls: 20 },
            ToolStat { tool_name: "Edit".into(), calls: 10 },
        ];
        stats.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.tool_name.cmp(&b.tool_name)));
        assert_eq!(stats[0].tool_name, "Bash");
        assert_eq!(stats[1].tool_name, "Edit");
        assert_eq!(stats[2].tool_name, "Read");
    }

    #[test]
    fn daily_stat_zero_tokens_zero_cost() {
        let stat = DailyStat { date: "2026-03-17".into(), tokens: 0, cost_usd: 0.0, events: 0 };
        assert_eq!(stat.tokens, 0);
        assert_eq!(stat.cost_usd, 0.0);
    }
}
