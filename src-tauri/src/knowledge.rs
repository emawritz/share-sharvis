use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntry {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub source_url: Option<String>,
    pub source_type: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub starred: bool,
}

/// Create the knowledge table. Called from db::create_tables or as a migration.
pub fn ensure_knowledge_table(conn: &rusqlite::Connection) {
    crate::db::run_migration(
        conn,
        100,
        "CREATE TABLE IF NOT EXISTS knowledge (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            source_url TEXT,
            source_type TEXT NOT NULL DEFAULT 'note',
            tags TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            starred INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_source_type ON knowledge(source_type);
        CREATE INDEX IF NOT EXISTS idx_knowledge_starred ON knowledge(starred);
        CREATE INDEX IF NOT EXISTS idx_knowledge_created_at ON knowledge(created_at);",
    );
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_knowledge(
    db: tauri::State<'_, crate::db::Db>,
    title: String,
    content: String,
    source_url: Option<String>,
    source_type: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<u64, String> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    ensure_knowledge_table(&conn);

    let st = source_type.unwrap_or_else(|| "note".into());
    let tags_json = serde_json::to_string(&tags.unwrap_or_default()).unwrap_or_else(|_| "[]".into());
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO knowledge (title, content, source_url, source_type, tags, created_at, starred)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![title, content, source_url, st, tags_json, now],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid() as u64;
    Ok(id)
}

#[tauri::command]
pub fn get_knowledge(
    db: tauri::State<'_, crate::db::Db>,
    limit: Option<u32>,
    offset: Option<u32>,
    source_type_filter: Option<String>,
    search_query: Option<String>,
) -> Vec<KnowledgeEntry> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    ensure_knowledge_table(&conn);

    let lim = limit.unwrap_or(50) as i64;
    let off = offset.unwrap_or(0) as i64;

    let mut conditions: Vec<String> = Vec::new();
    let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(ref st) = source_type_filter {
        conditions.push(format!("source_type = ?{param_idx}"));
        bind_values.push(Box::new(st.clone()));
        param_idx += 1;
    }
    if let Some(ref q) = search_query {
        let pattern = format!("%{q}%");
        conditions.push(format!(
            "(LOWER(title) LIKE LOWER(?{p}) OR LOWER(content) LIKE LOWER(?{p}))",
            p = param_idx
        ));
        bind_values.push(Box::new(pattern));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let lim_idx = param_idx;
    let off_idx = param_idx + 1;
    let sql = format!(
        "SELECT id, title, content, source_url, source_type, tags, created_at, starred
         FROM knowledge
         {}
         ORDER BY created_at DESC
         LIMIT ?{lim_idx} OFFSET ?{off_idx}",
        where_clause
    );

    bind_values.push(Box::new(lim));
    bind_values.push(Box::new(off));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_values.iter().map(|b| b.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map(params_refs.as_slice(), map_row);
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn delete_knowledge(
    db: tauri::State<'_, crate::db::Db>,
    id: u64,
) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    conn.execute("DELETE FROM knowledge WHERE id = ?1", params![id as i64])
        .map(|n| n > 0)
        .unwrap_or(false)
}

#[tauri::command]
pub fn star_knowledge(
    db: tauri::State<'_, crate::db::Db>,
    id: u64,
) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    // Toggle starred status
    conn.execute(
        "UPDATE knowledge SET starred = CASE WHEN starred = 0 THEN 1 ELSE 0 END WHERE id = ?1",
        params![id as i64],
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

#[tauri::command]
pub fn search_knowledge(
    db: tauri::State<'_, crate::db::Db>,
    query: String,
    limit: Option<usize>,
) -> Vec<KnowledgeEntry> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    ensure_knowledge_table(&conn);

    let lim = limit.unwrap_or(50) as i64;
    let pattern = format!("%{query}%");

    let sql = "SELECT id, title, content, source_url, source_type, tags, created_at, starred
               FROM knowledge
               WHERE LOWER(title) LIKE LOWER(?1) OR LOWER(content) LIKE LOWER(?1) OR LOWER(tags) LIKE LOWER(?1)
               ORDER BY created_at DESC
               LIMIT ?2";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map(params![pattern, lim], map_row);
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeEntry> {
    let id: i64 = row.get(0)?;
    let title: String = row.get(1)?;
    let content: String = row.get(2)?;
    let source_url: Option<String> = row.get(3)?;
    let source_type: String = row.get(4)?;
    let tags_json: String = row.get::<_, String>(5).unwrap_or_else(|_| "[]".into());
    let created_at: String = row.get(6)?;
    let starred: i64 = row.get::<_, i64>(7).unwrap_or(0);

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(KnowledgeEntry {
        id: id as u64,
        title,
        content,
        source_url,
        source_type,
        tags,
        created_at,
        starred: starred != 0,
    })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        // Create _migrations table so run_migration works
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )
        .unwrap();
        ensure_knowledge_table(&conn);
        conn
    }

    fn insert_entry(conn: &Connection, title: &str, content: &str, source_type: &str, tags: &[&str]) -> i64 {
        let tags_json = serde_json::to_string(&tags).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO knowledge (title, content, source_url, source_type, tags, created_at, starred)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, 0)",
            params![title, content, source_type, tags_json, now],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_insert_and_read() {
        let conn = setup_db();
        let id = insert_entry(&conn, "Test Note", "Some content", "note", &["rust", "test"]);
        assert!(id > 0);

        let mut stmt = conn
            .prepare("SELECT id, title, content, source_url, source_type, tags, created_at, starred FROM knowledge WHERE id = ?1")
            .unwrap();
        let entries: Vec<KnowledgeEntry> = stmt
            .query_map(params![id], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Test Note");
        assert_eq!(entries[0].tags, vec!["rust", "test"]);
        assert!(!entries[0].starred);
    }

    #[test]
    fn test_star_toggle() {
        let conn = setup_db();
        let id = insert_entry(&conn, "Star Me", "Content", "note", &[]);

        // Toggle on
        conn.execute(
            "UPDATE knowledge SET starred = CASE WHEN starred = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )
        .unwrap();

        let starred: i64 = conn
            .query_row("SELECT starred FROM knowledge WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(starred, 1);

        // Toggle off
        conn.execute(
            "UPDATE knowledge SET starred = CASE WHEN starred = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )
        .unwrap();

        let starred: i64 = conn
            .query_row("SELECT starred FROM knowledge WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(starred, 0);
    }

    #[test]
    fn test_delete() {
        let conn = setup_db();
        let id = insert_entry(&conn, "Delete Me", "Content", "note", &[]);

        let deleted = conn
            .execute("DELETE FROM knowledge WHERE id = ?1", params![id])
            .map(|n| n > 0)
            .unwrap_or(false);
        assert!(deleted);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_search_by_title_and_content() {
        let conn = setup_db();
        insert_entry(&conn, "Rust async patterns", "Tokio runtime overview", "research", &["rust"]);
        insert_entry(&conn, "Python ML", "TensorFlow guide", "research", &["python"]);

        let pattern = "%rust%";
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content, source_url, source_type, tags, created_at, starred
                 FROM knowledge
                 WHERE LOWER(title) LIKE LOWER(?1) OR LOWER(content) LIKE LOWER(?1) OR LOWER(tags) LIKE LOWER(?1)
                 ORDER BY created_at DESC LIMIT 50",
            )
            .unwrap();
        let results: Vec<KnowledgeEntry> = stmt
            .query_map(params![pattern], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        // Both should match: first by title, second would not match "rust"
        // Actually only the first matches (title has "Rust", tags has "rust")
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust async patterns");
    }
}
