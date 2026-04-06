use std::collections::HashSet;
use std::fs;
use std::process::Command;

use crate::types::{shell_escape, Activity, AgentDetail, AgentInfo, LogEntry};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn run_cmd(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// JSONL path derivation from repo path
// ---------------------------------------------------------------------------

/// Derive the Claude JSONL project directory from a repo path.
/// Example: "/Users/jane/projects/my-app"
///       -> "/Users/jane/.claude/projects/-Users-jane-projects-my-app/"
pub fn repo_path_to_jsonl_dir(repo_path: &str, is_local: bool, remote_home: Option<&str>) -> String {
    let expanded = if let Some(rest) = repo_path.strip_prefix("~/") {
        if is_local {
            let home = dirs::home_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            format!("{}/{}", home, rest)
        } else {
            let rh = remote_home.unwrap_or("/home/ema");
            format!("{}/{}", rh, rest)
        }
    } else {
        repo_path.to_string()
    };

    // Replace / with - (skip leading /)
    let dir_name = expanded.trim_start_matches('/').replace('/', "-");

    if is_local {
        let home = dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        format!("{}/.claude/projects/-{}/", home, dir_name)
    } else {
        let rh = remote_home.unwrap_or("/home/ema");
        format!("{}/.claude/projects/-{}/", rh, dir_name)
    }
}

// ---------------------------------------------------------------------------
// Generic JSONL finder
// ---------------------------------------------------------------------------

pub fn get_latest_jsonl(dir: &str) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    let mut files: Vec<(String, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".jsonl")
        })
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((
                e.path().to_string_lossy().to_string(),
                mtime,
            ))
        })
        .collect();

    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.first().map(|(path, _)| path.clone())
}

// ---------------------------------------------------------------------------
// JSONL line counter (efficient, no full load)
// ---------------------------------------------------------------------------

/// Count non-empty, non-whitespace lines in a JSONL file without loading the
/// whole file into memory.  Uses a buffered reader and counts non-blank lines
/// (each valid JSONL entry occupies exactly one line).
#[allow(dead_code)]
pub fn count_entries(path: &str) -> usize {
    use std::io::{BufRead, BufReader};
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.trim().is_empty())
        .count()
}

// ---------------------------------------------------------------------------
// JSONL activity parser
// ---------------------------------------------------------------------------

pub fn parse_jsonl_activity(jsonl_path: &str, max_lines: usize) -> Vec<Activity> {
    let max_lines = max_lines.max(1);
    let data = run_cmd("tail", &[&format!("-{}", max_lines), jsonl_path]);
    parse_raw_activity(&data)
}

pub fn parse_raw_activity(data: &str) -> Vec<Activity> {
    let mut activities = Vec::new();

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg = &parsed["message"];
        let role = msg["role"].as_str().unwrap_or("");
        let content = match msg["content"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        // Extract user prompts as "prompt" type
        if role == "user" {
            for block in content {
                let block_type = block["type"].as_str().unwrap_or("");
                if block_type == "text" {
                    let text = block["text"].as_str().unwrap_or("").trim();
                    if !text.is_empty() {
                        activities.push(Activity {
                            type_: "prompt".to_string(),
                            name: None,
                            detail: None,
                            content: Some(text.chars().take(2000).collect()),
                        });
                    }
                }
            }
            continue;
        }

        for block in content {
            let block_type = block["type"].as_str().unwrap_or("");
            if block_type == "text" && role == "assistant" {
                let text = block["text"].as_str().unwrap_or("").trim();
                if !text.is_empty() {
                    activities.push(Activity {
                        type_: "text".to_string(),
                        name: None,
                        detail: None,
                        content: Some(text.chars().take(2000).collect()),
                    });
                }
            } else if block_type == "tool_use" {
                let name = block["name"].as_str().unwrap_or("unknown");
                let input = &block["input"];
                let detail = match name {
                    "Bash" => {
                        let desc = input["description"].as_str().unwrap_or("");
                        let cmd = input["command"].as_str().unwrap_or("");
                        if !desc.is_empty() {
                            desc.to_string()
                        } else {
                            cmd.chars().take(300).collect::<String>()
                        }
                    }
                    "Read" | "Edit" | "Write" => {
                        input["file_path"].as_str().unwrap_or("").to_string()
                    }
                    "Grep" => {
                        let pat = input["pattern"].as_str().unwrap_or("");
                        let path = input["path"].as_str().unwrap_or("");
                        if path.is_empty() {
                            format!("\"{}\"", pat)
                        } else {
                            format!("\"{}\" in {}", pat, path)
                        }
                    }
                    "Glob" => input["pattern"].as_str().unwrap_or("").to_string(),
                    "Agent" => {
                        let desc = input["description"].as_str().unwrap_or("");
                        let prompt = input["prompt"].as_str().unwrap_or("");
                        if !desc.is_empty() {
                            desc.chars().take(200).collect::<String>()
                        } else {
                            prompt.chars().take(200).collect::<String>()
                        }
                    }
                    "ToolSearch" => {
                        let query = input["query"].as_str().unwrap_or("");
                        format!("\"{}\"", query)
                    }
                    _ => serde_json::to_string(input)
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect::<String>(),
                };
                activities.push(Activity {
                    type_: "tool".to_string(),
                    name: Some(name.to_string()),
                    detail: Some(detail),
                    content: None,
                });
            } else if block_type == "tool_result" {
                // Also capture tool results for more context
                let is_error = block["is_error"].as_bool().unwrap_or(false);
                if is_error {
                    let err_content = block["content"].as_str()
                        .or_else(|| {
                            block["content"].as_array().and_then(|arr| {
                                arr.first().and_then(|b| b["text"].as_str())
                            })
                        })
                        .unwrap_or("");
                    if !err_content.is_empty() {
                        activities.push(Activity {
                            type_: "text".to_string(),
                            name: Some("error".to_string()),
                            detail: None,
                            content: Some(err_content.chars().take(500).collect()),
                        });
                    }
                }
            }
        }
    }

    let len = activities.len();
    let start = len.saturating_sub(80);
    activities[start..].to_vec()
}

// ---------------------------------------------------------------------------
// Remote activity via SSH
// ---------------------------------------------------------------------------

pub fn get_remote_activity(host: &str, jsonl_dir: &str, max_lines: usize) -> Vec<Activity> {
    let result = run_cmd(
        "ssh",
        &[
            "-o",
            "ConnectTimeout=3",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            host,
            &format!(
                "JSONL=$(ls -t {}*.jsonl 2>/dev/null | head -1); tail -{} \"$JSONL\" 2>/dev/null",
                shell_escape(jsonl_dir), max_lines
            ),
        ],
    );
    parse_raw_activity(&result)
}

// ---------------------------------------------------------------------------
// Skill extraction from JSONL
// ---------------------------------------------------------------------------

pub fn extract_skills_from_jsonl(raw: &str) -> Vec<String> {
    let mut skills: HashSet<String> = HashSet::new();
    let lines: Vec<&str> = raw.lines().collect();
    let start = if lines.len() > 50 { lines.len() - 50 } else { 0 };

    for line in &lines[start..] {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg = &parsed["message"];
        let content = match msg["content"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        for block in content {
            if block["type"].as_str() != Some("tool_use") {
                continue;
            }
            let name = block["name"].as_str().unwrap_or("");
            let input = &block["input"];

            if name == "ToolSearch" {
                if let Some(query) = input["query"].as_str() {
                    if let Some(selected) = query.strip_prefix("select:") {
                        for s in selected.split(',') {
                            let s = s.trim();
                            if !s.is_empty() {
                                skills.insert(s.to_string());
                            }
                        }
                    }
                }
            } else if name == "Skill" {
                if let Some(skill_name) = input["name"].as_str().or_else(|| input["skillName"].as_str()) {
                    skills.insert(skill_name.to_string());
                }
            }
        }
    }

    let mut result: Vec<String> = skills.into_iter().collect();
    result.sort();
    result
}

// ---------------------------------------------------------------------------
// Subagent detection
// ---------------------------------------------------------------------------

/// Check if a subagent JSONL file indicates the agent is still running.
/// Reads the last ~4KB to find the last message. If it's an "assistant" message
/// and the file hasn't been modified recently, it's done.
fn is_subagent_still_running(path: &std::path::Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let meta = match file.metadata() {
        Ok(m) => m,
        Err(_) => return false,
    };
    let size = meta.len();
    if size == 0 { return false; }

    // Read last 4KB
    let read_from = size.saturating_sub(4096);
    let _ = file.seek(SeekFrom::Start(read_from));
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf);

    // Find the last valid JSONL line
    let mut last_role = "";
    let mut has_result = false;
    for line in buf.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let role = parsed["message"]["role"].as_str().unwrap_or("");
            if !role.is_empty() && last_role.is_empty() {
                last_role = if role == "assistant" { "assistant" } else { "user" };
            }
            // Check if content has "result" type blocks (tool results = agent returning)
            if let Some(content) = parsed["message"]["content"].as_array() {
                for block in content {
                    if block["type"].as_str() == Some("tool_result") {
                        has_result = true;
                    }
                }
            }
            // Only check the last 2 messages
            if last_role == "assistant" || has_result {
                break;
            }
        }
    }

    // Simple heuristic: if file was modified in last 30s, assume still running
    let now = std::time::SystemTime::now();
    if let Ok(mtime) = meta.modified() {
        let age = now.duration_since(mtime).unwrap_or_default().as_secs();
        if age <= 30 {
            return true;
        }
    }

    // If older than 30s and last role is assistant, it's probably done
    if last_role == "assistant" {
        return false;
    }

    // Default: if modified in last 90s but we can't determine, assume running
    true
}

// ---------------------------------------------------------------------------
// Agent info detection
// ---------------------------------------------------------------------------

pub fn get_local_agent_info(jsonl_dir: &str) -> AgentInfo {
    let active_files = get_active_jsonl_files(jsonl_dir, 300);
    let agent_count = active_files.len();
    let mut all_skills: HashSet<String> = HashSet::new();
    let mut subagent_count: usize = 0;
    let now = std::time::SystemTime::now();

    for (path, session_id, _) in &active_files {
        if let Ok(raw) = std::fs::read_to_string(path) {
            for s in extract_skills_from_jsonl(&raw) {
                all_skills.insert(s);
            }
        }
        // Count active subagent JSONL files in {session_id}/subagents/
        let subagents_dir = format!("{}{}/subagents", jsonl_dir, session_id);
        if let Ok(entries) = fs::read_dir(&subagents_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".jsonl") || name.contains("compact") { continue; }
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let age = now.duration_since(mtime).unwrap_or_default().as_secs();
                        if age <= 90 {
                            let sub_path = entry.path();
                            let still_running = is_subagent_still_running(&sub_path);
                            if still_running {
                                subagent_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut skills: Vec<String> = all_skills.into_iter().collect();
    skills.sort();
    // Total = main JSONL sessions + active subagent files
    AgentInfo { agent_count: agent_count + subagent_count, skills }
}

pub fn get_remote_agent_info(host: &str, jsonl_dir: &str, pixel_raw: &str) -> AgentInfo {
    let jsonl_dir_escaped = shell_escape(jsonl_dir);
    let cmd = format!(
        "main=$(find {} -maxdepth 1 -name '*.jsonl' -mmin -5 2>/dev/null | wc -l); \
         subs=$(find {} -path '*/subagents/*.jsonl' ! -name '*compact*' -mmin -1.5 2>/dev/null | wc -l); \
         echo $((main + subs))",
        jsonl_dir_escaped, jsonl_dir_escaped
    );
    let result = run_cmd(
        "ssh",
        &["-o", "ConnectTimeout=3", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", host, &cmd],
    );
    let agent_count = result.trim().parse::<usize>().unwrap_or(0);
    let skills = extract_skills_from_jsonl(pixel_raw);
    AgentInfo { agent_count, skills }
}

// ---------------------------------------------------------------------------
// Active JSONL file discovery
// ---------------------------------------------------------------------------

pub fn get_active_jsonl_files(dir: &str, max_age_secs: u64) -> Vec<(String, String, u64)> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let now = std::time::SystemTime::now();
    let mut files: Vec<(String, String, u64)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".jsonl")
        })
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            let age = now.duration_since(mtime).ok()?.as_secs();
            if age <= max_age_secs {
                let name = e.file_name().to_string_lossy().to_string();
                let session_id = name.trim_end_matches(".jsonl").to_string();
                let path = format!("{}{}", dir, name);
                Some((path, session_id, age))
            } else {
                None
            }
        })
        .collect();
    files.sort_by_key(|f| f.2);
    files
}

// ---------------------------------------------------------------------------
// Last activity extraction from JSONL lines
// ---------------------------------------------------------------------------

pub fn extract_last_activity(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut last_tool: Option<String> = None;
    let mut last_detail: Option<String> = None;
    let mut last_text: Option<String> = None;

    for line in raw.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg = &parsed["message"];
        let role = msg["role"].as_str().unwrap_or("");
        let content = match msg["content"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        if role != "assistant" {
            continue;
        }

        for block in content.iter().rev() {
            let block_type = block["type"].as_str().unwrap_or("");
            if block_type == "tool_use" && last_tool.is_none() {
                let name = block["name"].as_str().unwrap_or("unknown");
                let input = &block["input"];
                let detail = match name {
                    "Bash" => input["description"]
                        .as_str()
                        .or_else(|| input["command"].as_str())
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>(),
                    "Read" | "Edit" | "Write" => {
                        input["file_path"].as_str().unwrap_or("").to_string()
                    }
                    "Grep" => format!("\"{}\"", input["pattern"].as_str().unwrap_or("")),
                    "Agent" => input["description"]
                        .as_str()
                        .or_else(|| input["prompt"].as_str())
                        .unwrap_or("")
                        .chars()
                        .take(80)
                        .collect::<String>(),
                    _ => serde_json::to_string(input)
                        .unwrap_or_default()
                        .chars()
                        .take(60)
                        .collect::<String>(),
                };
                last_tool = Some(name.to_string());
                last_detail = Some(detail);
            } else if block_type == "text" && last_text.is_none() {
                let text = block["text"].as_str().unwrap_or("").trim();
                if !text.is_empty() {
                    last_text = Some(text.chars().take(120).collect());
                }
            }
            if last_tool.is_some() && last_text.is_some() {
                return (last_tool, last_detail, last_text);
            }
        }

        if last_tool.is_some() || last_text.is_some() {
            return (last_tool, last_detail, last_text);
        }
    }

    (last_tool, last_detail, last_text)
}

// ---------------------------------------------------------------------------
// Agent detail detection
// ---------------------------------------------------------------------------

pub fn get_local_agent_details(jsonl_dir: &str) -> Vec<AgentDetail> {
    let files = get_active_jsonl_files(jsonl_dir, 300); // 5 min
    files
        .into_iter()
        .filter_map(|(path, session_id, age)| {
            let raw = run_cmd("tail", &["-20", &path]);
            if raw.trim().is_empty() {
                return None;
            }
            let (last_tool, last_detail, last_text) = extract_last_activity(&raw);
            Some(AgentDetail {
                session_id,
                last_tool,
                last_detail,
                last_text,
                seconds_ago: age,
            })
        })
        .collect()
}

pub fn get_remote_agent_details(host: &str, jsonl_dir: &str) -> Vec<AgentDetail> {
    let raw = run_cmd(
        "ssh",
        &[
            "-o", "ConnectTimeout=3",
            "-o", "ServerAliveInterval=5",
            "-o", "ServerAliveCountMax=3",
            host,
            &format!("for f in $(ls -t {}*.jsonl 2>/dev/null); do age=$(($(date +%s) - $(stat -c %Y \"$f\" 2>/dev/null || echo 0))); if [ $age -le 300 ]; then sid=$(basename \"$f\" .jsonl); echo \"===AGENT:${{sid}}:${{age}}===\"; tail -20 \"$f\"; fi; done", shell_escape(jsonl_dir)),
        ],
    );

    let mut agents = Vec::new();
    let mut current_sid = String::new();
    let mut current_age: u64 = 0;
    let mut current_lines = Vec::new();

    for line in raw.lines() {
        if line.starts_with("===AGENT:") && line.ends_with("===") {
            // Flush previous
            if !current_sid.is_empty() && !current_lines.is_empty() {
                let block = current_lines.join("\n");
                let (last_tool, last_detail, last_text) = extract_last_activity(&block);
                agents.push(AgentDetail {
                    session_id: current_sid.clone(),
                    last_tool,
                    last_detail,
                    last_text,
                    seconds_ago: current_age,
                });
            }
            // Strip "===AGENT:" prefix (9 ASCII bytes) and "===" suffix (3 ASCII bytes) safely
            let inner = line
                .strip_prefix("===AGENT:")
                .and_then(|s| s.strip_suffix("==="))
                .unwrap_or("");
            let parts: Vec<&str> = inner.splitn(2, ':').collect();
            current_sid = parts.first().unwrap_or(&"").to_string();
            current_age = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            current_lines.clear();
        } else if current_lines.len() < 100 {
            current_lines.push(line.to_string());
        }
    }
    // Flush last
    if !current_sid.is_empty() && !current_lines.is_empty() {
        let block = current_lines.join("\n");
        let (last_tool, last_detail, last_text) = extract_last_activity(&block);
        agents.push(AgentDetail {
            session_id: current_sid,
            last_tool,
            last_detail,
            last_text,
            seconds_ago: current_age,
        });
    }

    agents
}

// ---------------------------------------------------------------------------
// JSONL log parsing with pagination
// ---------------------------------------------------------------------------

pub fn parse_jsonl_log(raw: &str, offset: usize, limit: usize) -> (Vec<LogEntry>, usize) {
    let mut entries: Vec<LogEntry> = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let timestamp = parsed["timestamp"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let msg = &parsed["message"];
        let role = msg["role"].as_str().unwrap_or("");
        let content = match msg["content"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        if role == "user" {
            for block in content {
                let block_type = block["type"].as_str().unwrap_or("");
                if block_type == "text" {
                    let text = block["text"].as_str().unwrap_or("").trim();
                    if !text.is_empty() {
                        entries.push(LogEntry {
                            timestamp: timestamp.clone(),
                            type_: "prompt".to_string(),
                            tool_name: None,
                            input_summary: Some(text.chars().take(500).collect()),
                            output_summary: None,
                            duration_ms: None,
                            is_error: false,
                        });
                    }
                }
            }
            continue;
        }

        for block in content {
            let block_type = block["type"].as_str().unwrap_or("");
            if block_type == "tool_use" {
                let name = block["name"].as_str().unwrap_or("unknown").to_string();
                let input = &block["input"];
                let input_summary = serde_json::to_string(input)
                    .unwrap_or_default()
                    .chars()
                    .take(500)
                    .collect::<String>();
                entries.push(LogEntry {
                    timestamp: timestamp.clone(),
                    type_: "tool_use".to_string(),
                    tool_name: Some(name),
                    input_summary: Some(input_summary),
                    output_summary: None,
                    duration_ms: None,
                    is_error: false,
                });
            } else if block_type == "tool_result" {
                let is_error = block["is_error"].as_bool().unwrap_or(false);
                let output = block["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| {
                        block["content"].as_array().and_then(|arr| {
                            arr.first().and_then(|b| b["text"].as_str().map(|s| s.to_string()))
                        })
                    })
                    .unwrap_or_default();
                let output_summary: String = output.chars().take(500).collect();
                entries.push(LogEntry {
                    timestamp: timestamp.clone(),
                    type_: "tool_result".to_string(),
                    tool_name: None,
                    input_summary: None,
                    output_summary: Some(output_summary),
                    duration_ms: None,
                    is_error,
                });
            } else if block_type == "text" && role == "assistant" {
                let text = block["text"].as_str().unwrap_or("").trim();
                if !text.is_empty() {
                    entries.push(LogEntry {
                        timestamp: timestamp.clone(),
                        type_: "text".to_string(),
                        tool_name: None,
                        input_summary: None,
                        output_summary: Some(text.chars().take(500).collect()),
                        duration_ms: None,
                        is_error: false,
                    });
                }
            }
        }
    }

    let total = entries.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let page = entries[start..end].to_vec();
    (page, total)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- repo_path_to_jsonl_dir --

    #[test]
    fn jsonl_dir_local_absolute_path() {
        // Local absolute path: /Users/jane/myproject
        // Expected: <home>/.claude/projects/-Users-ema-myproject/
        let result = repo_path_to_jsonl_dir("/Users/jane/myproject", true, None);
        let home = dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let expected = format!("{}/.claude/projects/-Users-ema-myproject/", home);
        assert_eq!(result, expected);
    }

    #[test]
    fn jsonl_dir_local_trailing_slash_is_not_stripped_by_function() {
        // The function does not strip trailing slashes from the input path itself;
        // a trailing slash on the input becomes a trailing dash in the dir_name.
        // Test that the output is still a valid string (no panic).
        let result = repo_path_to_jsonl_dir("/Users/jane/myproject/", true, None);
        assert!(result.contains(".claude/projects/"));
        assert!(result.ends_with('/'));
    }

    #[test]
    fn jsonl_dir_remote_tilde_path_with_home_dir() {
        // Remote path with ~ and explicit home_dir
        let result = repo_path_to_jsonl_dir("~/Projects/my-frontend", false, Some("/home/worker"));
        // expanded: /home/worker/Projects/my-frontend
        // dir_name: home-worker-Projects-my-frontend
        let expected = "/home/worker/.claude/projects/-home-worker-Projects-my-frontend/";
        assert_eq!(result, expected);
    }

    #[test]
    fn jsonl_dir_remote_tilde_path_default_home() {
        // Remote path with ~ but no explicit home_dir — should use default "/home/ema"
        let result = repo_path_to_jsonl_dir("~/myproject", false, None);
        let expected = "/home/user/.claude/projects/-home-ema-myproject/";
        assert_eq!(result, expected);
    }

    #[test]
    fn jsonl_dir_slashes_become_dashes() {
        // Dots in the path are preserved; slashes → dashes (excluding leading slash)
        let result = repo_path_to_jsonl_dir("/Users/jane/my-app", false, Some("/home/ema"));
        // expanded: /Users/jane/my-app  (absolute, no ~ expansion)
        // dir_name: Users-ema-my-app
        let expected = "/home/user/.claude/projects/-Users-ema-my-app/";
        assert_eq!(result, expected);
    }

    #[test]
    fn jsonl_dir_local_tilde_path_expands_to_real_home() {
        let result = repo_path_to_jsonl_dir("~/myproject", true, None);
        let home = dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // expanded path: <home>/myproject → dir_name: <home_stripped_slash>-myproject
        assert!(result.starts_with(&home));
        assert!(result.contains(".claude/projects/"));
        assert!(result.ends_with('/'));
    }

    // -------------------------------------------------------------------------
    // parse_raw_activity
    // -------------------------------------------------------------------------

    fn make_tool_use_line(role: &str, tool_name: &str, input_json: &str) -> String {
        format!(
            r#"{{"message":{{"role":"{}","content":[{{"type":"tool_use","name":"{}","input":{}}}]}}}}"#,
            role, tool_name, input_json
        )
    }

    fn make_text_line(role: &str, text: &str) -> String {
        format!(
            r#"{{"message":{{"role":"{}","content":[{{"type":"text","text":"{}"}}]}}}}"#,
            role, text
        )
    }

    #[test]
    fn parse_raw_activity_empty_input_returns_empty() {
        let result = parse_raw_activity("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_raw_activity_only_blank_lines_returns_empty() {
        let result = parse_raw_activity("\n\n   \n\t\n");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_raw_activity_malformed_json_skipped_no_panic() {
        let data = "not json at all\n{broken\n{\"also\":\"bad\"\n";
        let result = parse_raw_activity(data);
        assert!(result.is_empty(), "malformed lines must be silently skipped");
    }

    #[test]
    fn parse_raw_activity_user_text_becomes_prompt_type() {
        let line = make_text_line("user", "Hello, do something");
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_, "prompt");
        assert_eq!(result[0].content.as_deref(), Some("Hello, do something"));
        assert!(result[0].name.is_none());
    }

    #[test]
    fn parse_raw_activity_assistant_text_becomes_text_type() {
        let line = make_text_line("assistant", "I will help you");
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_, "text");
        assert_eq!(result[0].content.as_deref(), Some("I will help you"));
    }

    #[test]
    fn parse_raw_activity_tool_use_bash_with_description() {
        let input = r#"{"description":"Run tests","command":"cargo test"}"#;
        let line = make_tool_use_line("assistant", "Bash", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_, "tool");
        assert_eq!(result[0].name.as_deref(), Some("Bash"));
        // description preferred over command
        assert_eq!(result[0].detail.as_deref(), Some("Run tests"));
    }

    #[test]
    fn parse_raw_activity_tool_use_bash_falls_back_to_command() {
        let input = r#"{"command":"cargo build"}"#;
        let line = make_tool_use_line("assistant", "Bash", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].detail.as_deref(), Some("cargo build"));
    }

    #[test]
    fn parse_raw_activity_tool_use_read_uses_file_path() {
        let input = r#"{"file_path":"/src/main.rs"}"#;
        let line = make_tool_use_line("assistant", "Read", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("/src/main.rs"));
    }

    #[test]
    fn parse_raw_activity_tool_use_grep_with_path() {
        let input = r#"{"pattern":"TODO","path":"/src"}"#;
        let line = make_tool_use_line("assistant", "Grep", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("\"TODO\" in /src"));
    }

    #[test]
    fn parse_raw_activity_tool_use_grep_without_path() {
        let input = r#"{"pattern":"FIXME"}"#;
        let line = make_tool_use_line("assistant", "Grep", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("\"FIXME\""));
    }

    #[test]
    fn parse_raw_activity_tool_use_glob_uses_pattern() {
        let input = r#"{"pattern":"**/*.rs"}"#;
        let line = make_tool_use_line("assistant", "Glob", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("**/*.rs"));
    }

    #[test]
    fn parse_raw_activity_tool_use_agent_uses_description() {
        let input = r#"{"description":"Analyze codebase","prompt":"Go look at everything"}"#;
        let line = make_tool_use_line("assistant", "Agent", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("Analyze codebase"));
    }

    #[test]
    fn parse_raw_activity_tool_use_agent_falls_back_to_prompt() {
        let input = r#"{"prompt":"Go look at everything"}"#;
        let line = make_tool_use_line("assistant", "Agent", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("Go look at everything"));
    }

    #[test]
    fn parse_raw_activity_tool_use_toolsearch_quotes_query() {
        let input = r#"{"query":"select:Read,Edit"}"#;
        let line = make_tool_use_line("assistant", "ToolSearch", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].detail.as_deref(), Some("\"select:Read,Edit\""));
    }

    #[test]
    fn parse_raw_activity_unknown_tool_serializes_input() {
        let input = r#"{"foo":"bar"}"#;
        let line = make_tool_use_line("assistant", "MyCustomTool", input);
        let result = parse_raw_activity(&line);
        assert_eq!(result[0].type_, "tool");
        assert_eq!(result[0].name.as_deref(), Some("MyCustomTool"));
        // detail is serde_json re-serialization of the input, truncated to 200 chars
        assert!(result[0].detail.as_deref().unwrap_or("").contains("bar"));
    }

    #[test]
    fn parse_raw_activity_tool_error_result_emits_text_error_activity() {
        // tool_result with is_error is processed in the non-user (assistant) branch
        let line = r#"{"message":{"role":"assistant","content":[{"type":"tool_result","is_error":true,"content":"Permission denied"}]}}"#;
        let result = parse_raw_activity(line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_, "text");
        assert_eq!(result[0].name.as_deref(), Some("error"));
        assert_eq!(result[0].content.as_deref(), Some("Permission denied"));
    }

    #[test]
    fn parse_raw_activity_non_error_tool_result_not_emitted() {
        // Non-error tool_result blocks are silently dropped (only errors are surfaced)
        // In user role: user branch skips tool_result blocks entirely
        let user_line = r#"{"message":{"role":"user","content":[{"type":"tool_result","is_error":false,"content":"OK"}]}}"#;
        let result = parse_raw_activity(user_line);
        assert!(result.is_empty());
        // In assistant role: tool_result with is_error=false is also not emitted
        let asst_line = r#"{"message":{"role":"assistant","content":[{"type":"tool_result","is_error":false,"content":"OK"}]}}"#;
        let result2 = parse_raw_activity(asst_line);
        assert!(result2.is_empty());
    }

    #[test]
    fn parse_raw_activity_utf8_and_emojis_no_panic() {
        // Chinese chars, emojis, tildes — must not panic
        let text = "你好 🌍 ñoño résumé";
        let line = make_text_line("assistant", text);
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].type_, "text");
    }

    #[test]
    fn parse_raw_activity_very_long_content_truncated_at_2000_chars() {
        let long_text = "A".repeat(5000);
        let line = make_text_line("user", &long_text);
        let result = parse_raw_activity(&line);
        assert_eq!(result.len(), 1);
        // content capped at 2000 chars
        assert_eq!(result[0].content.as_deref().unwrap_or("").len(), 2000);
    }

    #[test]
    fn parse_raw_activity_missing_content_array_skipped() {
        // message has no content array → line is skipped entirely
        let line = r#"{"message":{"role":"assistant","content":"just a string"}}"#;
        let result = parse_raw_activity(line);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_raw_activity_multiple_lines_mixed() {
        let tool_line = make_tool_use_line("assistant", "Read", r#"{"file_path":"/a.rs"}"#);
        let text_line = make_text_line("assistant", "Done reading");
        let user_line = make_text_line("user", "Please read that file");
        let data = format!("{}\n{}\n{}", user_line, tool_line, text_line);
        let result = parse_raw_activity(&data);
        // Should have: prompt, tool, text
        assert_eq!(result.len(), 3);
        let types: Vec<&str> = result.iter().map(|a| a.type_.as_str()).collect();
        assert_eq!(types, vec!["prompt", "tool", "text"]);
    }

    #[test]
    fn parse_raw_activity_capped_at_80_items() {
        // More than 80 lines → only last 80 activities returned
        let mut lines = Vec::new();
        for i in 0..100 {
            lines.push(make_text_line("assistant", &format!("Message {}", i)));
        }
        let data = lines.join("\n");
        let result = parse_raw_activity(&data);
        assert_eq!(result.len(), 80);
        // The last item should be from the last line (message 99)
        assert!(result.last().unwrap().content.as_deref().unwrap_or("").contains("99"));
    }

    // -------------------------------------------------------------------------
    // extract_last_activity
    // -------------------------------------------------------------------------

    #[test]
    fn extract_last_activity_empty_returns_nones() {
        let (tool, detail, text) = extract_last_activity("");
        assert!(tool.is_none());
        assert!(detail.is_none());
        assert!(text.is_none());
    }

    #[test]
    fn extract_last_activity_only_malformed_returns_nones() {
        let (tool, detail, text) = extract_last_activity("not json\n{broken}");
        assert!(tool.is_none());
        assert!(detail.is_none());
        assert!(text.is_none());
    }

    #[test]
    fn extract_last_activity_picks_last_tool_from_assistant() {
        let line = make_tool_use_line("assistant", "Edit", r#"{"file_path":"/src/lib.rs"}"#);
        let (tool, detail, text) = extract_last_activity(&line);
        assert_eq!(tool.as_deref(), Some("Edit"));
        assert_eq!(detail.as_deref(), Some("/src/lib.rs"));
        assert!(text.is_none());
    }

    #[test]
    fn extract_last_activity_ignores_user_role_messages() {
        // user role messages should be skipped — only assistant messages are checked
        let user_line = make_text_line("user", "Hey agent");
        let (tool, detail, text) = extract_last_activity(&user_line);
        assert!(tool.is_none());
        assert!(detail.is_none());
        assert!(text.is_none());
    }

    #[test]
    fn extract_last_activity_text_truncated_at_120_chars() {
        let long = "B".repeat(300);
        let line = make_text_line("assistant", &long);
        let (_, _, text) = extract_last_activity(&line);
        assert_eq!(text.as_deref().unwrap_or("").len(), 120);
    }

    #[test]
    fn extract_last_activity_bash_tool_detail_truncated_at_100_chars() {
        let long_cmd = "C".repeat(200);
        let input = format!(r#"{{"command":"{}"}}"#, long_cmd);
        let line = make_tool_use_line("assistant", "Bash", &input);
        let (tool, detail, _) = extract_last_activity(&line);
        assert_eq!(tool.as_deref(), Some("Bash"));
        assert_eq!(detail.as_deref().unwrap_or("").len(), 100);
    }

    // -------------------------------------------------------------------------
    // extract_skills_from_jsonl
    // -------------------------------------------------------------------------

    #[test]
    fn extract_skills_empty_input_returns_empty() {
        let result = extract_skills_from_jsonl("");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_skills_from_tool_search_select_prefix() {
        let line = make_tool_use_line(
            "assistant",
            "ToolSearch",
            r#"{"query":"select:Read,Edit,Grep","max_results":5}"#,
        );
        let result = extract_skills_from_jsonl(&line);
        assert!(result.contains(&"Read".to_string()));
        assert!(result.contains(&"Edit".to_string()));
        assert!(result.contains(&"Grep".to_string()));
    }

    #[test]
    fn extract_skills_tool_search_non_select_prefix_ignored() {
        let line = make_tool_use_line(
            "assistant",
            "ToolSearch",
            r#"{"query":"notebook jupyter"}"#,
        );
        let result = extract_skills_from_jsonl(&line);
        // "notebook jupyter" doesn't start with "select:" so nothing extracted
        assert!(result.is_empty());
    }

    #[test]
    fn extract_skills_from_skill_tool_name_field() {
        let line = make_tool_use_line(
            "assistant",
            "Skill",
            r#"{"name":"pdf","args":"some args"}"#,
        );
        let result = extract_skills_from_jsonl(&line);
        assert!(result.contains(&"pdf".to_string()));
    }

    #[test]
    fn extract_skills_from_skill_tool_skill_name_field() {
        let line = make_tool_use_line(
            "assistant",
            "Skill",
            r#"{"skillName":"commit"}"#,
        );
        let result = extract_skills_from_jsonl(&line);
        assert!(result.contains(&"commit".to_string()));
    }

    #[test]
    fn extract_skills_deduplicates_and_sorts() {
        // Two lines using same skill
        let line1 = make_tool_use_line("assistant", "Skill", r#"{"name":"pdf"}"#);
        let line2 = make_tool_use_line("assistant", "Skill", r#"{"name":"pdf"}"#);
        let line3 = make_tool_use_line("assistant", "Skill", r#"{"name":"commit"}"#);
        let data = format!("{}\n{}\n{}", line1, line2, line3);
        let result = extract_skills_from_jsonl(&data);
        // no duplicates
        assert_eq!(result.iter().filter(|s| s.as_str() == "pdf").count(), 1);
        // sorted
        assert_eq!(result, vec!["commit", "pdf"]);
    }

    #[test]
    fn extract_skills_only_scans_last_50_lines() {
        // Put a skill beyond the 50-line window (at start) and one inside it
        let early_line = make_tool_use_line("assistant", "Skill", r#"{"name":"early_skill"}"#);
        let filler: String = (0..50)
            .map(|i| make_text_line("assistant", &format!("filler {}", i)))
            .collect::<Vec<_>>()
            .join("\n");
        let late_line = make_tool_use_line("assistant", "Skill", r#"{"name":"late_skill"}"#);
        let data = format!("{}\n{}\n{}", early_line, filler, late_line);
        let result = extract_skills_from_jsonl(&data);
        // late_skill is inside window; early_skill may or may not be depending on count
        assert!(result.contains(&"late_skill".to_string()));
        // early_skill is before the 50-line window so it should NOT be present
        assert!(!result.contains(&"early_skill".to_string()));
    }

    // -------------------------------------------------------------------------
    // parse_jsonl_log
    // -------------------------------------------------------------------------

    fn make_log_line(role: &str, block_type: &str, extra: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-01-01T00:00:00Z","message":{{"role":"{}","content":[{{"type":"{}",{}}}]}}}}"#,
            role, block_type, extra
        )
    }

    #[test]
    fn parse_jsonl_log_empty_returns_empty_with_zero_total() {
        let (entries, total) = parse_jsonl_log("", 0, 100);
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn parse_jsonl_log_malformed_lines_skipped() {
        let (entries, total) = parse_jsonl_log("not json\n{bad}", 0, 100);
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn parse_jsonl_log_user_text_creates_prompt_entry() {
        let line = make_log_line("user", "text", r#""text":"Do the thing""#);
        let (entries, total) = parse_jsonl_log(&line, 0, 100);
        assert_eq!(total, 1);
        assert_eq!(entries[0].type_, "prompt");
        assert_eq!(entries[0].input_summary.as_deref(), Some("Do the thing"));
        assert_eq!(entries[0].timestamp, "2026-01-01T00:00:00Z");
        assert!(!entries[0].is_error);
    }

    #[test]
    fn parse_jsonl_log_tool_use_entry_has_correct_fields() {
        let line = make_log_line(
            "assistant",
            "tool_use",
            r#""name":"Read","input":{"file_path":"/foo.rs"}"#,
        );
        let (entries, total) = parse_jsonl_log(&line, 0, 100);
        assert_eq!(total, 1);
        assert_eq!(entries[0].type_, "tool_use");
        assert_eq!(entries[0].tool_name.as_deref(), Some("Read"));
        assert!(entries[0].input_summary.as_deref().unwrap_or("").contains("foo.rs"));
    }

    #[test]
    fn parse_jsonl_log_tool_result_error_sets_is_error() {
        // tool_result blocks are processed in the non-user (assistant) branch
        let line = r#"{"timestamp":"T","message":{"role":"assistant","content":[{"type":"tool_result","is_error":true,"content":"Command failed"}]}}"#;
        let (entries, total) = parse_jsonl_log(line, 0, 100);
        assert_eq!(total, 1);
        assert_eq!(entries[0].type_, "tool_result");
        assert!(entries[0].is_error);
        assert_eq!(entries[0].output_summary.as_deref(), Some("Command failed"));
    }

    #[test]
    fn parse_jsonl_log_tool_result_non_error() {
        // Non-error tool_result in assistant role is recorded without is_error flag
        let line = r#"{"timestamp":"T","message":{"role":"assistant","content":[{"type":"tool_result","is_error":false,"content":"Success output"}]}}"#;
        let (entries, total) = parse_jsonl_log(line, 0, 100);
        assert_eq!(total, 1);
        assert!(!entries[0].is_error);
        assert_eq!(entries[0].output_summary.as_deref(), Some("Success output"));
    }

    #[test]
    fn parse_jsonl_log_tool_result_in_user_role_is_skipped() {
        // user role only processes text blocks; tool_result is silently skipped
        let line = r#"{"timestamp":"T","message":{"role":"user","content":[{"type":"tool_result","is_error":true,"content":"ignored"}]}}"#;
        let (entries, total) = parse_jsonl_log(line, 0, 100);
        assert_eq!(total, 0);
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_jsonl_log_assistant_text_creates_text_entry() {
        let line = make_log_line("assistant", "text", r#""text":"I finished the task""#);
        let (entries, total) = parse_jsonl_log(&line, 0, 100);
        assert_eq!(total, 1);
        assert_eq!(entries[0].type_, "text");
        assert_eq!(entries[0].output_summary.as_deref(), Some("I finished the task"));
    }

    #[test]
    fn parse_jsonl_log_pagination_offset_and_limit() {
        // Build 10 user text entries
        let lines: String = (0..10)
            .map(|i| make_log_line("user", "text", &format!(r#""text":"item {}""#, i)))
            .collect::<Vec<_>>()
            .join("\n");
        let (entries, total) = parse_jsonl_log(&lines, 3, 4);
        assert_eq!(total, 10);
        assert_eq!(entries.len(), 4);
        // item 3 should be first
        assert!(entries[0].input_summary.as_deref().unwrap_or("").contains("item 3"));
    }

    #[test]
    fn parse_jsonl_log_offset_beyond_total_returns_empty_slice() {
        let line = make_log_line("user", "text", r#""text":"only one""#);
        let (entries, total) = parse_jsonl_log(&line, 100, 10);
        assert_eq!(total, 1);
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_jsonl_log_utf8_non_ascii_no_panic() {
        let line = make_log_line("user", "text", r#""text":"こんにちは 🦀 café""#);
        let (entries, total) = parse_jsonl_log(&line, 0, 10);
        assert_eq!(total, 1);
        assert!(entries[0].input_summary.as_deref().unwrap_or("").contains("🦀"));
    }

    #[test]
    fn parse_jsonl_log_very_long_text_truncated_at_500_chars() {
        let long_text = "Z".repeat(2000);
        let line = make_log_line("user", "text", &format!(r#""text":"{}""#, long_text));
        let (entries, _) = parse_jsonl_log(&line, 0, 100);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].input_summary.as_deref().unwrap_or("").len(), 500);
    }

    // -------------------------------------------------------------------------
    // get_remote_agent_details — SSH output parsing (pure section)
    // -------------------------------------------------------------------------

    // The agent-block parsing inside get_remote_agent_details is not a standalone
    // pure fn, but extract_last_activity IS its core. We exercise the parsing
    // protocol via extract_last_activity which is called on each block.

    #[test]
    fn extract_last_activity_returns_both_tool_and_text_when_present() {
        // Build a single JSONL line that has both a tool_use and a text block
        let line = format!(
            r#"{{"message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Write","input":{{"file_path":"/out.rs"}}}},{{"type":"text","text":"Writing now"}}]}}}}"#
        );
        let (tool, detail, text) = extract_last_activity(&line);
        assert_eq!(tool.as_deref(), Some("Write"));
        assert_eq!(detail.as_deref(), Some("/out.rs"));
        assert_eq!(text.as_deref(), Some("Writing now"));
    }

    // -------------------------------------------------------------------------
    // count_entries
    // -------------------------------------------------------------------------

    /// Write content to a temp file and return its path.
    fn write_tmp(content: &str) -> String {
        let path = std::env::temp_dir()
            .join(format!("jarvis_test_{}.jsonl", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()))
            .to_string_lossy()
            .to_string();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn count_entries_nonexistent_file_returns_zero() {
        assert_eq!(count_entries("/nonexistent/path/that/does/not/exist.jsonl"), 0);
    }

    #[test]
    fn count_entries_counts_non_blank_lines() {
        let content = "{\"a\":1}\n\n{\"b\":2}\n   \n{\"c\":3}\n";
        let path = write_tmp(content);
        assert_eq!(count_entries(&path), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn count_entries_empty_file_returns_zero() {
        let path = write_tmp("");
        assert_eq!(count_entries(&path), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn count_entries_single_line_no_newline_returns_one() {
        let path = write_tmp("{\"x\":1}"); // no trailing newline
        assert_eq!(count_entries(&path), 1);
        let _ = std::fs::remove_file(&path);
    }
}
