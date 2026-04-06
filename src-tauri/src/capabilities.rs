use std::collections::HashSet;
use std::fs;

use serde::Serialize;

use crate::jsonl;
use crate::machines::MachineRegistry;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineCapabilities {
    pub machine_id: String,
    pub machine_name: String,
    pub plugins: Vec<PluginInfo>,
    pub agents: Vec<AgentFile>,
    pub mcps: Vec<String>,
    pub skills_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFile {
    pub filename: String,
    pub content_preview: String,
}

// ---------------------------------------------------------------------------
// Local capability gathering
// ---------------------------------------------------------------------------

fn get_local_capabilities(machine_id: &str, machine_name: &str) -> MachineCapabilities {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // 1. Read settings.json for enabled plugins
    let plugins = read_local_plugins(&home);

    // 2. Read agent .md files
    let agents = read_local_agents(&home);

    // 3. Extract MCP prefixes and skills from JSONL files
    let (mcps, skills) = extract_local_mcps_and_skills(&home);

    MachineCapabilities {
        machine_id: machine_id.to_string(),
        machine_name: machine_name.to_string(),
        plugins,
        agents,
        mcps,
        skills_used: skills,
    }
}

fn read_local_plugins(home: &str) -> Vec<PluginInfo> {
    let mut plugins = Vec::new();

    // Read settings.json for enabledPlugins
    let settings_path = format!("{}/.claude/settings.json", home);
    if let Ok(data) = fs::read_to_string(&settings_path) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
            if let Some(enabled) = parsed.get("enabledPlugins").and_then(|v| v.as_object()) {
                for (name, val) in enabled {
                    let is_enabled = val.as_bool().unwrap_or(false);
                    plugins.push(PluginInfo {
                        name: name.clone(),
                        enabled: is_enabled,
                    });
                }
            }
        }
    }

    // Also list installed plugin dirs
    let plugin_cache = format!("{}/.claude/plugins/cache/claude-plugins-official/", home);
    if let Ok(entries) = fs::read_dir(&plugin_cache) {
        let installed: HashSet<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        // Add any installed plugins not already in the enabledPlugins list
        let existing: HashSet<String> = plugins.iter().map(|p| p.name.clone()).collect();
        for dir_name in installed {
            if !existing.contains(&dir_name) {
                plugins.push(PluginInfo {
                    name: dir_name,
                    enabled: false,
                });
            }
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn read_local_agents(home: &str) -> Vec<AgentFile> {
    let agents_dir = format!("{}/.claude/agents/", home);
    let mut agents = Vec::new();

    if let Ok(entries) = fs::read_dir(&agents_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") {
                continue;
            }
            let path = entry.path();
            let preview = fs::read_to_string(&path)
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            agents.push(AgentFile {
                filename: name,
                content_preview: preview,
            });
        }
    }

    agents.sort_by(|a, b| a.filename.cmp(&b.filename));
    agents
}

fn extract_local_mcps_and_skills(home: &str) -> (Vec<String>, Vec<String>) {
    let projects_dir = format!("{}/.claude/projects/", home);
    let mut mcp_prefixes: HashSet<String> = HashSet::new();
    let mut all_skills: HashSet<String> = HashSet::new();

    // Scan all project JSONL dirs for recent files
    if let Ok(proj_entries) = fs::read_dir(&projects_dir) {
        for proj_entry in proj_entries.filter_map(|e| e.ok()) {
            if !proj_entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
            let proj_path = proj_entry.path();
            let proj_dir = format!("{}/", proj_path.to_string_lossy());

            // Get latest JSONL in this project dir
            if let Some(jsonl_path) = jsonl::get_latest_jsonl(&proj_dir) {
                if let Ok(raw) = fs::read_to_string(&jsonl_path) {
                    // Extract MCP tool prefixes
                    extract_mcp_prefixes_from_raw(&raw, &mut mcp_prefixes);
                    // Extract skills
                    for skill in jsonl::extract_skills_from_jsonl(&raw) {
                        all_skills.insert(skill);
                    }
                }
            }
        }
    }

    let mut mcps: Vec<String> = mcp_prefixes.into_iter().collect();
    mcps.sort();
    let mut skills: Vec<String> = all_skills.into_iter().collect();
    skills.sort();
    (mcps, skills)
}

fn extract_mcp_prefixes_from_raw(raw: &str, prefixes: &mut HashSet<String>) {
    // Only scan last ~100 lines for performance
    let lines: Vec<&str> = raw.lines().collect();
    let start = if lines.len() > 100 { lines.len() - 100 } else { 0 };

    for line in &lines[start..] {
        let line = line.trim();
        if line.is_empty() || !line.contains("mcp__") {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let content = match parsed["message"]["content"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        for block in content {
            if block["type"].as_str() != Some("tool_use") {
                continue;
            }
            if let Some(name) = block["name"].as_str() {
                if name.starts_with("mcp__") {
                    // Extract prefix: mcp__<server>__<tool> -> mcp__<server>
                    let parts: Vec<&str> = name.splitn(3, "__").collect();
                    if parts.len() >= 2 {
                        let prefix = format!("mcp__{}", parts[1]);
                        prefixes.insert(prefix);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Remote capability gathering
// ---------------------------------------------------------------------------

fn get_remote_capabilities(
    host: &str,
    machine_id: &str,
    machine_name: &str,
) -> MachineCapabilities {
    let ssh_cmd = concat!(
        "cat ~/.claude/settings.json 2>/dev/null",
        "\necho '===SECTION==='",
        "\nls ~/.claude/agents/*.md 2>/dev/null",
        "\necho '===SECTION==='",
        "\nfor f in ~/.claude/agents/*.md; do [ -f \"$f\" ] && echo \"===FILE:$(basename $f)===\"; head -c 200 \"$f\" 2>/dev/null; done",
        "\necho '===SECTION==='",
        "\nls ~/.claude/plugins/cache/claude-plugins-official/ 2>/dev/null",
    );

    let raw = jsonl::run_cmd(
        "ssh",
        &[
            "-o", "ConnectTimeout=5",
            "-o", "ServerAliveInterval=30",
            "-o", "ServerAliveCountMax=20",
            "-o", "StrictHostKeyChecking=no",
            host,
            ssh_cmd,
        ],
    );

    let sections: Vec<&str> = raw.split("===SECTION===").collect();

    // Section 0: settings.json
    let plugins = parse_remote_plugins(
        sections.first().unwrap_or(&""),
        sections.get(3).unwrap_or(&""),
    );

    // Section 1+2: agent files
    let agents = parse_remote_agents(sections.get(2).unwrap_or(&""));

    MachineCapabilities {
        machine_id: machine_id.to_string(),
        machine_name: machine_name.to_string(),
        plugins,
        agents,
        mcps: Vec::new(),       // Would need JSONL access; skip for remote
        skills_used: Vec::new(), // Would need JSONL access; skip for remote
    }
}

fn parse_remote_plugins(settings_section: &str, plugin_dirs_section: &str) -> Vec<PluginInfo> {
    let mut plugins = Vec::new();
    let trimmed = settings_section.trim();

    if !trimmed.is_empty() {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(enabled) = parsed.get("enabledPlugins").and_then(|v| v.as_object()) {
                for (name, val) in enabled {
                    plugins.push(PluginInfo {
                        name: name.clone(),
                        enabled: val.as_bool().unwrap_or(false),
                    });
                }
            }
        }
    }

    // Add installed plugin dirs not already listed
    let existing: HashSet<String> = plugins.iter().map(|p| p.name.clone()).collect();
    for line in plugin_dirs_section.lines() {
        let dir = line.trim();
        if !dir.is_empty() && !existing.contains(dir) {
            plugins.push(PluginInfo {
                name: dir.to_string(),
                enabled: false,
            });
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn parse_remote_agents(agents_section: &str) -> Vec<AgentFile> {
    let mut agents = Vec::new();
    let mut current_file = String::new();
    let mut current_content = Vec::new();

    for line in agents_section.lines() {
        if line.starts_with("===FILE:") && line.ends_with("===") {
            // Flush previous
            if !current_file.is_empty() {
                agents.push(AgentFile {
                    filename: current_file.clone(),
                    content_preview: current_content.join("\n").chars().take(200).collect(),
                });
            }
            current_file = line
                .strip_prefix("===FILE:")
                .and_then(|s| s.strip_suffix("==="))
                .unwrap_or("")
                .to_string();
            current_content.clear();
        } else {
            current_content.push(line.to_string());
        }
    }
    // Flush last
    if !current_file.is_empty() {
        agents.push(AgentFile {
            filename: current_file,
            content_preview: current_content.join("\n").chars().take(200).collect(),
        });
    }

    agents.sort_by(|a, b| a.filename.cmp(&b.filename));
    agents
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

/// Fetch capabilities from ALL enabled machines in parallel.
#[tauri::command]
pub async fn get_machine_capabilities(
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<Vec<MachineCapabilities>, String> {
    // Clone machines out before any I/O
    let machines_snapshot: Vec<(String, String, String)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines
            .values()
            .filter(|m| m.enabled)
            .map(|m| (m.id.clone(), m.name.clone(), m.host.clone()))
            .collect()
    };

    // Spawn one thread per machine so SSH calls run in parallel
    let handles: Vec<_> = machines_snapshot
        .into_iter()
        .map(|(id, name, host)| {
            std::thread::spawn(move || {
                if host == "local" {
                    get_local_capabilities(&id, &name)
                } else {
                    get_remote_capabilities(&host, &id, &name)
                }
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(caps) = handle.join() {
            results.push(caps); // silently skip panicking threads
        }
    }

    Ok(results)
}

/// Fetch capabilities for a single machine by its ID.
#[tauri::command]
pub async fn get_single_machine_capabilities(
    machine_id: String,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<MachineCapabilities, String> {
    let machine_info: Option<(String, String, String)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.get(&machine_id).map(|m| (m.id.clone(), m.name.clone(), m.host.clone()))
    };

    let (id, name, host) = machine_info
        .ok_or_else(|| format!("Machine '{}' not found", machine_id))?;

    let caps = tokio::task::spawn_blocking(move || {
        if host == "local" {
            get_local_capabilities(&id, &name)
        } else {
            get_remote_capabilities(&host, &id, &name)
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {}", e))?;

    Ok(caps)
}
