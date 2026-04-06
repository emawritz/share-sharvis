use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    pub id: u64,
    pub from: String,
    pub to: String,
    pub category: String,
    pub content: String,
    pub timestamp: String,
    pub read: bool,
    pub tags: Vec<String>,
    /// Sub-category for memory messages (e.g. "architecture", "decisions", "todo").
    /// Only meaningful when `category == "memory"`.
    #[serde(default)]
    pub memory_category: Option<String>,
    /// Pinned memories appear at the top of memory lists.
    #[serde(default)]
    pub pin: bool,
}

/// A view over an `AgentMessage` that is specifically a team memory entry.
/// Returned by memory-focused commands to expose the memory-specific fields
/// without the noise of the general message fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemory {
    pub id: u64,
    pub from: String,
    pub content: String,
    pub timestamp: String,
    pub tags: Vec<String>,
    /// Organisational category (e.g. "architecture", "decisions", "todo").
    #[serde(default)]
    pub category: Option<String>,
    /// Pinned memories stay at the top of the list.
    #[serde(default)]
    pub pin: bool,
}

impl TeamMemory {
    fn from_msg(msg: &AgentMessage) -> Self {
        TeamMemory {
            id: msg.id,
            from: msg.from.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp.clone(),
            tags: msg.tags.clone(),
            category: msg.memory_category.clone(),
            pin: msg.pin,
        }
    }
}

const MAX_MESSAGES: usize = 200;
const MAX_MEMORIES: usize = 500;

fn messages_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    home.join(".config/jarvis/messages.json")
}

fn load_messages() -> Vec<AgentMessage> {
    let path = messages_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_messages(msgs: &[AgentMessage]) {
    let path = messages_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(msgs) {
        let _ = std::fs::write(&path, json);
    }
}

fn next_id(msgs: &[AgentMessage]) -> u64 {
    msgs.iter().map(|m| m.id).max().unwrap_or(0) + 1
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// -- Public API used by other modules --

pub fn get_team_context_for(target: &str) -> String {
    let msgs = load_messages();
    let mut parts = Vec::new();

    let memories: Vec<&AgentMessage> = msgs.iter()
        .filter(|m| m.category == "memory")
        .rev()
        .take(20)
        .collect();
    if !memories.is_empty() {
        parts.push("[MEMORIA DEL EQUIPO]".to_string());
        for mem in memories.iter().rev() {
            let tags = if mem.tags.is_empty() { String::new() } else { format!(" [{}]", mem.tags.join(", ")) };
            parts.push(format!("- {}{}", mem.content, tags));
        }
        parts.push(String::new());
    }

    let unread: Vec<&AgentMessage> = msgs.iter()
        .filter(|m| !m.read && m.category != "memory")
        .filter(|m| m.to == target || m.to == "all")
        .filter(|m| m.from != target)
        .collect();
    if !unread.is_empty() {
        parts.push(format!("[MENSAJES NO LEIDOS para {}]", target));
        for msg in &unread {
            parts.push(format!("- {} ({}): {}", msg.from, msg.category, msg.content));
        }
        parts.push(String::new());
    }

    if parts.is_empty() {
        return String::new();
    }
    parts.join("\n")
}

pub fn extract_memories_from_output(output: &str, from: &str) {
    let mut msgs = load_messages();
    let mut added = false;
    for line in output.lines() {
        let trimmed = line.trim();
        let content_opt = trimmed
            .strip_prefix("[MEMORY]")
            .or_else(|| trimmed.strip_prefix("[MEMORIA]"))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(content) = content_opt {
            let id = next_id(&msgs);
            msgs.push(AgentMessage {
                id,
                from: from.to_string(),
                to: "all".to_string(),
                category: "memory".to_string(),
                content: content.to_string(),
                timestamp: now_iso(),
                read: false,
                tags: vec!["auto".to_string()],
                memory_category: None,
                pin: false,
            });
            added = true;
        }
    }
    if added {
        let memory_count = msgs.iter().filter(|m| m.category == "memory").count();
        if memory_count > MAX_MEMORIES {
            let excess = memory_count - MAX_MEMORIES;
            let mut removed = 0;
            msgs.retain(|m| {
                if m.category == "memory" && removed < excess {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
        save_messages(&msgs);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: u64, category: &str) -> AgentMessage {
        AgentMessage {
            id,
            from: "test".to_string(),
            to: "all".to_string(),
            category: category.to_string(),
            content: "hello".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            read: false,
            tags: vec![],
            memory_category: None,
            pin: false,
        }
    }

    fn make_memory(id: u64, content: &str, mem_cat: Option<&str>, pin: bool) -> AgentMessage {
        AgentMessage {
            id,
            from: "user".to_string(),
            to: "all".to_string(),
            category: "memory".to_string(),
            content: content.to_string(),
            timestamp: format!("2026-01-{:02}T00:00:00Z", id),
            read: false,
            tags: vec![],
            memory_category: mem_cat.map(|s| s.to_string()),
            pin,
        }
    }

    // next_id: empty slice → 1
    #[test]
    fn next_id_empty_returns_one() {
        let msgs: Vec<AgentMessage> = vec![];
        assert_eq!(next_id(&msgs), 1);
    }

    // next_id: returns max id + 1
    #[test]
    fn next_id_returns_max_plus_one() {
        let msgs = vec![make_msg(3, "info"), make_msg(7, "info"), make_msg(2, "info")];
        assert_eq!(next_id(&msgs), 8);
    }

    // next_id: single message
    #[test]
    fn next_id_single_message() {
        let msgs = vec![make_msg(42, "info")];
        assert_eq!(next_id(&msgs), 43);
    }

    // extract_memories parsing: lines with [MEMORY] prefix are extracted
    // We test the parsing logic in isolation using a helper that mirrors what
    // extract_memories_from_output does — determining which lines would be extracted.
    fn lines_that_would_be_extracted(output: &str) -> Vec<String> {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("[MEMORY]")
                    .or_else(|| trimmed.strip_prefix("[MEMORIA]"))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .collect()
    }

    #[test]
    fn memory_extraction_memory_prefix() {
        let output = "Some text\n[MEMORY] Important insight\nMore text";
        let extracted = lines_that_would_be_extracted(output);
        assert_eq!(extracted, vec!["Important insight"]);
    }

    #[test]
    fn memory_extraction_memoria_prefix() {
        let output = "[MEMORIA] Recordar que el puerto es 8080";
        let extracted = lines_that_would_be_extracted(output);
        assert_eq!(extracted, vec!["Recordar que el puerto es 8080"]);
    }

    #[test]
    fn memory_extraction_no_matching_lines() {
        let output = "Just a normal output\nNo memory markers here";
        let extracted = lines_that_would_be_extracted(output);
        assert!(extracted.is_empty());
    }

    #[test]
    fn memory_extraction_empty_content_after_prefix_is_skipped() {
        let output = "[MEMORY]   \n[MEMORY] valid content";
        let extracted = lines_that_would_be_extracted(output);
        // Empty/whitespace-only content after prefix should be skipped
        assert_eq!(extracted, vec!["valid content"]);
    }

    #[test]
    fn memory_extraction_multiple_entries() {
        let output = "[MEMORY] first\n[MEMORIA] second\nskip this\n[MEMORY] third";
        let extracted = lines_that_would_be_extracted(output);
        assert_eq!(extracted, vec!["first", "second", "third"]);
    }

    #[test]
    fn memory_extraction_trims_whitespace() {
        let output = "  [MEMORY]   padded content   ";
        let extracted = lines_that_would_be_extracted(output);
        assert_eq!(extracted, vec!["padded content"]);
    }

    // -- TeamMemory / new fields --

    #[test]
    fn team_memory_from_msg_maps_fields() {
        let msg = make_memory(1, "use postgres", Some("architecture"), true);
        let tm = TeamMemory::from_msg(&msg);
        assert_eq!(tm.id, 1);
        assert_eq!(tm.content, "use postgres");
        assert_eq!(tm.category, Some("architecture".to_string()));
        assert!(tm.pin);
    }

    #[test]
    fn team_memory_from_msg_no_category() {
        let msg = make_memory(2, "plain note", None, false);
        let tm = TeamMemory::from_msg(&msg);
        assert!(tm.category.is_none());
        assert!(!tm.pin);
    }

    #[test]
    fn search_filters_by_content_case_insensitive() {
        // Build a local list and apply the same filter logic used by search_team_memories.
        let msgs = vec![
            make_memory(1, "Use PostgreSQL for the main DB", Some("architecture"), false),
            make_memory(2, "Redis for caching", Some("architecture"), false),
            make_memory(3, "TODO: write tests", Some("todo"), false),
        ];
        let query = "postgresql";
        let query_lower = query.to_lowercase();
        let results: Vec<TeamMemory> = msgs
            .iter()
            .filter(|m| m.content.to_lowercase().contains(&query_lower))
            .map(TeamMemory::from_msg)
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
    }

    #[test]
    fn search_filters_by_category() {
        let msgs = vec![
            make_memory(1, "arch decision", Some("architecture"), false),
            make_memory(2, "todo item", Some("todo"), false),
            make_memory(3, "another arch note", Some("architecture"), false),
        ];
        let cat = Some("architecture".to_string());
        let results: Vec<TeamMemory> = msgs
            .iter()
            .filter(|m| {
                if let Some(ref c) = cat {
                    m.memory_category.as_deref() == Some(c.as_str())
                } else {
                    true
                }
            })
            .map(TeamMemory::from_msg)
            .collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn get_memory_categories_deduplication() {
        let msgs = vec![
            make_memory(1, "a", Some("architecture"), false),
            make_memory(2, "b", Some("todo"), false),
            make_memory(3, "c", Some("architecture"), false),
            make_memory(4, "d", None, false),
        ];
        let mut cats: Vec<String> = msgs
            .iter()
            .filter(|m| m.category == "memory")
            .filter_map(|m| m.memory_category.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        assert_eq!(cats, vec!["architecture", "todo"]);
    }

    #[test]
    fn pinned_memories_sort_before_unpinned() {
        let msgs = vec![
            make_memory(1, "regular", None, false),
            make_memory(2, "pinned", None, true),
            make_memory(3, "also regular", None, false),
        ];
        let mut memories: Vec<TeamMemory> = msgs.iter().map(TeamMemory::from_msg).collect();
        memories.sort_by(|a, b| b.pin.cmp(&a.pin).then_with(|| b.timestamp.cmp(&a.timestamp)));
        // First entry must be the pinned one.
        assert!(memories[0].pin);
        assert_eq!(memories[0].id, 2);
    }

    #[test]
    fn pin_field_default_is_false() {
        let msg = make_msg(10, "memory");
        assert!(!msg.pin);
    }
}

// -- Tauri Commands --

#[tauri::command]
pub fn send_agent_message(
    from: String,
    to: String,
    category: String,
    content: String,
    tags: Vec<String>,
) -> Result<AgentMessage, String> {
    if content.len() > 50_000 {
        return Err("Content too large (max 50KB)".into());
    }
    if from.len() > 100 {
        return Err("From field too large (max 100 chars)".into());
    }
    if to.len() > 100 {
        return Err("To field too large (max 100 chars)".into());
    }
    if tags.len() > 20 {
        return Err("Too many tags (max 20)".into());
    }
    let mut msgs = load_messages();
    let id = next_id(&msgs);
    let msg = AgentMessage {
        id,
        from,
        to,
        category,
        content,
        timestamp: now_iso(),
        read: false,
        tags,
        memory_category: None,
        pin: false,
    };
    msgs.push(msg.clone());

    let non_memory_count = msgs.iter().filter(|m| m.category != "memory").count();
    if non_memory_count > MAX_MESSAGES {
        let excess = non_memory_count - MAX_MESSAGES;
        let mut removed = 0;
        msgs.retain(|m| {
            if m.category != "memory" && removed < excess {
                removed += 1;
                false
            } else {
                true
            }
        });
    }

    save_messages(&msgs);
    Ok(msg)
}

#[tauri::command]
pub fn get_agent_messages(
    target: Option<String>,
    unread_only: Option<bool>,
    category: Option<String>,
) -> Vec<AgentMessage> {
    let msgs = load_messages();
    msgs.into_iter()
        .filter(|m| {
            if let Some(ref t) = target {
                if m.to != *t && m.to != "all" && m.from != *t {
                    return false;
                }
            }
            if unread_only.unwrap_or(false) && m.read {
                return false;
            }
            if let Some(ref cat) = category {
                if m.category != *cat {
                    return false;
                }
            }
            true
        })
        .collect()
}

#[tauri::command]
pub fn mark_messages_read(target: String) -> bool {
    let mut msgs = load_messages();
    for msg in msgs.iter_mut() {
        if (msg.to == target || msg.to == "all") && !msg.read {
            msg.read = true;
        }
    }
    save_messages(&msgs);
    true
}

#[tauri::command]
pub fn clear_messages(category: Option<String>) -> bool {
    let mut msgs = load_messages();
    if let Some(cat) = category {
        msgs.retain(|m| m.category != cat);
    } else {
        msgs.retain(|m| m.category == "memory");
    }
    save_messages(&msgs);
    true
}

#[tauri::command]
pub fn save_team_memory(
    content: String,
    tags: Vec<String>,
    category: Option<String>,
) -> Result<TeamMemory, String> {
    if content.len() > 50_000 {
        return Err("Content too large (max 50KB)".into());
    }
    if tags.len() > 20 {
        return Err("Too many tags (max 20)".into());
    }
    let mut msgs = load_messages();
    let id = next_id(&msgs);
    let msg = AgentMessage {
        id,
        from: "user".into(),
        to: "all".into(),
        category: "memory".into(),
        content,
        timestamp: now_iso(),
        read: false,
        tags,
        memory_category: category,
        pin: false,
    };
    let result = TeamMemory::from_msg(&msg);
    msgs.push(msg);

    let memory_count = msgs.iter().filter(|m| m.category == "memory").count();
    if memory_count > MAX_MEMORIES {
        let excess = memory_count - MAX_MEMORIES;
        let mut removed = 0;
        msgs.retain(|m| {
            if m.category == "memory" && !m.pin && removed < excess {
                removed += 1;
                false
            } else {
                true
            }
        });
    }

    save_messages(&msgs);
    Ok(result)
}

#[tauri::command]
pub fn get_team_memories() -> Vec<TeamMemory> {
    let msgs = load_messages();
    let mut memories: Vec<TeamMemory> = msgs
        .iter()
        .filter(|m| m.category == "memory")
        .map(TeamMemory::from_msg)
        .collect();
    // Pinned memories first, then by most-recent timestamp.
    memories.sort_by(|a, b| b.pin.cmp(&a.pin).then_with(|| b.timestamp.cmp(&a.timestamp)));
    memories
}

#[tauri::command]
pub fn delete_team_memory(id: u64) -> bool {
    let mut msgs = load_messages();
    let before = msgs.len();
    msgs.retain(|m| m.id != id);
    save_messages(&msgs);
    msgs.len() < before
}

#[tauri::command]
pub fn get_team_context(target: String) -> String {
    get_team_context_for(&target)
}

#[tauri::command]
pub fn search_team_memories(query: String, category: Option<String>) -> Vec<TeamMemory> {
    let msgs = load_messages();
    let query_lower = query.to_lowercase();
    let mut memories: Vec<TeamMemory> = msgs
        .iter()
        .filter(|m| m.category == "memory")
        .filter(|m| {
            // Optional category filter
            if let Some(ref cat) = category {
                if m.memory_category.as_deref() != Some(cat.as_str()) {
                    return false;
                }
            }
            // Case-insensitive search across content and tags
            m.content.to_lowercase().contains(&query_lower)
                || m.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
        })
        .map(TeamMemory::from_msg)
        .collect();
    // Pinned first, then most-recent.
    memories.sort_by(|a, b| b.pin.cmp(&a.pin).then_with(|| b.timestamp.cmp(&a.timestamp)));
    memories
}

#[tauri::command]
pub fn get_memory_categories() -> Vec<String> {
    let msgs = load_messages();
    let mut cats: Vec<String> = msgs
        .iter()
        .filter(|m| m.category == "memory")
        .filter_map(|m| m.memory_category.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    cats.sort();
    cats
}

#[tauri::command]
pub fn pin_memory(id: u64, pinned: bool) -> Result<(), String> {
    let mut msgs = load_messages();
    let found = msgs.iter_mut().find(|m| m.id == id && m.category == "memory");
    match found {
        Some(msg) => {
            msg.pin = pinned;
            save_messages(&msgs);
            Ok(())
        }
        None => Err(format!("Memory with id {} not found", id)),
    }
}
