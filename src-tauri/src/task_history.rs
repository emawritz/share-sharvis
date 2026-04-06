use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHistoryEntry {
    pub id: u64,
    pub target: String,
    pub prompt: String,
    pub output: String,
    pub status: String,
    pub timestamp: String,
    pub duration_secs: u64,
}

pub fn save_to_history(app: &tauri::AppHandle, id: u64, target: &str, prompt: &str, output: &str, success: bool, duration_secs: u64) {
    use tauri::Manager;
    let db = app.state::<crate::db::Db>();
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_save_task_history(&conn, id, target, prompt, output, success, duration_secs);
}

#[tauri::command]
pub fn get_task_history(
    db: tauri::State<'_, crate::db::Db>,
    target: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Vec<TaskHistoryEntry> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let lim = limit.unwrap_or(50) as i64;
    let off = offset.unwrap_or(0) as i64;

    // Build WHERE clause with positional placeholders — values bound as params, never interpolated
    let mut conditions: Vec<&str> = Vec::new();
    if target.is_some() { conditions.push("target = ?1"); }
    if status.is_some() { conditions.push("status = ?2"); }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT task_id, target, prompt, status, output, duration_secs, completed_at
         FROM task_history
         {}
         ORDER BY completed_at DESC
         LIMIT ?3 OFFSET ?4",
        where_clause
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let target_val = target.as_deref().unwrap_or("");
    let status_val = status.as_deref().unwrap_or("");
    let rows = stmt.query_map(
        rusqlite::params![target_val, status_val, lim, off],
        |row| {
        let task_id: i64 = row.get(0)?;
        let t: String = row.get(1)?;
        let prompt: String = row.get(2)?;
        let st: String = row.get(3)?;
        let output: String = row.get(4)?;
        let dur: f64 = row.get::<_, f64>(5).unwrap_or(0.0);
        let completed_at: i64 = row.get(6)?;
        Ok(TaskHistoryEntry {
            id: task_id as u64,
            target: t,
            prompt,
            status: st,
            output,
            timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp(completed_at, 0)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339(),
            duration_secs: dur as u64,
        })
    });

    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn count_task_history(
    db: tauri::State<'_, crate::db::Db>,
    target: Option<String>,
    status: Option<String>,
) -> u32 {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());

    let mut conditions: Vec<&str> = Vec::new();
    if target.is_some() { conditions.push("target = ?1"); }
    if status.is_some() { conditions.push("status = ?2"); }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("SELECT COUNT(*) FROM task_history {}", where_clause);
    let target_val = target.as_deref().unwrap_or("");
    let status_val = status.as_deref().unwrap_or("");
    conn.query_row(&sql, rusqlite::params![target_val, status_val], |row| row.get::<_, i64>(0))
        .map(|n| n as u32)
        .unwrap_or(0)
}

#[tauri::command]
pub fn clear_task_history(db: tauri::State<'_, crate::db::Db>) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    conn.execute("DELETE FROM task_history", []).is_ok()
}

#[tauri::command]
pub fn search_task_history(
    db: tauri::State<'_, crate::db::Db>,
    query: String,
    limit: Option<usize>,
) -> Vec<TaskHistoryEntry> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let lim = limit.unwrap_or(50) as i64;
    // SQLite LIKE is case-insensitive for ASCII by default; wrap query in wildcards
    let pattern = format!("%{}%", query);

    let sql = "SELECT task_id, target, prompt, status, output, duration_secs, completed_at
               FROM task_history
               WHERE LOWER(prompt) LIKE LOWER(?1) OR LOWER(output) LIKE LOWER(?1)
               ORDER BY completed_at DESC
               LIMIT ?2";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map(rusqlite::params![pattern, lim], map_row);
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

#[tauri::command]
pub fn get_task_history_by_machine(
    db: tauri::State<'_, crate::db::Db>,
    machine_id: String,
    limit: Option<usize>,
) -> Vec<TaskHistoryEntry> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let lim = limit.unwrap_or(50) as i64;

    let sql = "SELECT task_id, target, prompt, status, output, duration_secs, completed_at
               FROM task_history
               WHERE target = ?1
               ORDER BY completed_at DESC
               LIMIT ?2";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt.query_map(rusqlite::params![machine_id, lim], map_row);
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

/// Shared row mapper used by multiple query functions.
fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskHistoryEntry> {
    let task_id: i64 = row.get(0)?;
    let target: String = row.get(1)?;
    let prompt: String = row.get(2)?;
    let status: String = row.get(3)?;
    let output: String = row.get(4)?;
    let dur: f64 = row.get::<_, f64>(5).unwrap_or(0.0);
    let completed_at: i64 = row.get(6)?;
    Ok(TaskHistoryEntry {
        id: task_id as u64,
        target,
        prompt,
        status,
        output,
        timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp(completed_at, 0)
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339(),
        duration_secs: dur as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE task_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL,
                target TEXT NOT NULL,
                prompt TEXT NOT NULL,
                status TEXT NOT NULL,
                output TEXT NOT NULL DEFAULT '',
                duration_secs REAL,
                completed_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    fn insert_entry(conn: &Connection, task_id: i64, target: &str, prompt: &str, output: &str, status: &str, completed_at: i64) {
        conn.execute(
            "INSERT INTO task_history (task_id, target, prompt, status, output, duration_secs, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![task_id, target, prompt, status, output, 5.0_f64, completed_at],
        )
        .unwrap();
    }

    #[test]
    fn test_map_row_converts_fields_correctly() {
        let conn = setup_db();
        insert_entry(&conn, 1, "atlas", "Deploy backend", "Done", "success", 1_700_000_000);

        let mut stmt = conn
            .prepare("SELECT task_id, target, prompt, status, output, duration_secs, completed_at FROM task_history")
            .unwrap();
        let entries: Vec<TaskHistoryEntry> = stmt
            .query_map([], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.id, 1);
        assert_eq!(e.target, "atlas");
        assert_eq!(e.prompt, "Deploy backend");
        assert_eq!(e.status, "success");
        assert_eq!(e.duration_secs, 5);
    }

    #[test]
    fn test_search_matches_prompt_case_insensitive() {
        let conn = setup_db();
        insert_entry(&conn, 1, "atlas", "Deploy Backend Service", "", "success", 1_700_000_001);
        insert_entry(&conn, 2, "pixel", "Run Tests", "", "success", 1_700_000_002);

        let pattern = format!("%{}%", "deploy");
        let mut stmt = conn
            .prepare("SELECT task_id, target, prompt, status, output, duration_secs, completed_at FROM task_history WHERE LOWER(prompt) LIKE LOWER(?1) OR LOWER(output) LIKE LOWER(?1) ORDER BY completed_at DESC LIMIT 50")
            .unwrap();
        let results: Vec<TaskHistoryEntry> = stmt
            .query_map(rusqlite::params![pattern], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].prompt, "Deploy Backend Service");
    }

    #[test]
    fn test_search_matches_output_field() {
        let conn = setup_db();
        insert_entry(&conn, 3, "atlas", "Some prompt", "Error: connection refused", "fail", 1_700_000_003);
        insert_entry(&conn, 4, "atlas", "Other prompt", "All tests passed", "success", 1_700_000_004);

        let pattern = format!("%{}%", "connection refused");
        let mut stmt = conn
            .prepare("SELECT task_id, target, prompt, status, output, duration_secs, completed_at FROM task_history WHERE LOWER(prompt) LIKE LOWER(?1) OR LOWER(output) LIKE LOWER(?1) ORDER BY completed_at DESC LIMIT 50")
            .unwrap();
        let results: Vec<TaskHistoryEntry> = stmt
            .query_map(rusqlite::params![pattern], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "fail");
    }

    #[test]
    fn test_filter_by_machine() {
        let conn = setup_db();
        insert_entry(&conn, 5, "atlas", "Task A", "", "success", 1_700_000_005);
        insert_entry(&conn, 6, "pixel", "Task B", "", "success", 1_700_000_006);
        insert_entry(&conn, 7, "atlas", "Task C", "", "fail", 1_700_000_007);

        let machine_id = "atlas";
        let mut stmt = conn
            .prepare("SELECT task_id, target, prompt, status, output, duration_secs, completed_at FROM task_history WHERE target = ?1 ORDER BY completed_at DESC LIMIT 50")
            .unwrap();
        let results: Vec<TaskHistoryEntry> = stmt
            .query_map(rusqlite::params![machine_id], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.target == "atlas"));
    }

    #[test]
    fn test_search_returns_empty_for_no_match() {
        let conn = setup_db();
        insert_entry(&conn, 8, "atlas", "Deploy frontend", "OK", "success", 1_700_000_008);

        let pattern = format!("%{}%", "xyznonexistent");
        let mut stmt = conn
            .prepare("SELECT task_id, target, prompt, status, output, duration_secs, completed_at FROM task_history WHERE LOWER(prompt) LIKE LOWER(?1) OR LOWER(output) LIKE LOWER(?1) ORDER BY completed_at DESC LIMIT 50")
            .unwrap();
        let results: Vec<TaskHistoryEntry> = stmt
            .query_map(rusqlite::params![pattern], map_row)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(results.is_empty());
    }
}
