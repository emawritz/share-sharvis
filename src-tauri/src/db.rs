use rusqlite::{Connection, Result, params};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db {
    pub conn: Mutex<Connection>,
}

impl Db {
    pub fn open() -> Result<Self> {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        create_tables(&conn)?;
        migrate_schema(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/jarvis/jarvis.db")
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            target TEXT NOT NULL,
            prompt TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            output TEXT NOT NULL DEFAULT '',
            started_at INTEGER,
            completed_at INTEGER,
            duration_secs REAL,
            orchestrate INTEGER NOT NULL DEFAULT 0,
            depends_on TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            trigger TEXT NOT NULL,
            condition TEXT,
            action TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            fire_count INTEGER NOT NULL DEFAULT 0,
            last_fired TEXT,
            priority INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS cron_jobs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            cron_expr TEXT NOT NULL,
            target TEXT NOT NULL,
            prompt TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_run TEXT,
            next_run TEXT,
            run_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS task_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL,
            target TEXT NOT NULL,
            prompt TEXT NOT NULL,
            status TEXT NOT NULL,
            output TEXT NOT NULL DEFAULT '',
            duration_secs REAL,
            completed_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS rule_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            trigger TEXT NOT NULL,
            result TEXT NOT NULL
        );
        ",
    )
}

/// Apply additive schema migrations for existing databases (ALTER TABLE for new columns).
fn migrate_schema(conn: &Connection) {
    // Add priority column to rules if it doesn't exist yet (existing DBs won't have it)
    let _ = conn.execute_batch(
        "ALTER TABLE rules ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;",
    );
}

// ---------------------------------------------------------------------------
// Task helpers
// ---------------------------------------------------------------------------

pub fn db_save_task_history(
    conn: &Connection,
    task_id: u64,
    target: &str,
    prompt: &str,
    output: &str,
    success: bool,
    duration_secs: u64,
) {
    let status = if success { "success" } else { "fail" };
    let completed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let output_trimmed: String = output.chars().take(5000).collect();
    // Insert + prune atomically so the table never grows unboundedly on crash
    if let Ok(tx) = conn.unchecked_transaction() {
        let _ = tx.execute(
            "INSERT INTO task_history (task_id, target, prompt, status, output, duration_secs, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                task_id as i64,
                target,
                prompt,
                status,
                output_trimmed,
                duration_secs as f64,
                completed_at,
            ],
        );
        let _ = tx.execute(
            "DELETE FROM task_history WHERE id NOT IN (
                SELECT id FROM task_history ORDER BY completed_at DESC LIMIT 200
             )",
            [],
        );
        let _ = tx.commit();
    }
}

// ---------------------------------------------------------------------------
// Rules helpers
// ---------------------------------------------------------------------------

pub fn db_load_rules(conn: &Connection) -> Vec<crate::rules::AutoRule> {
    let mut stmt = match conn.prepare(
        "SELECT id, name, trigger, condition, action, enabled, fire_count, last_fired, priority FROM rules ORDER BY priority ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |row| {
        let condition_str: Option<String> = row.get(3)?;
        let action_str: String = row.get(4)?;
        let enabled: i64 = row.get(5)?;
        let fire_count: i64 = row.get(6)?;
        let priority: i64 = row.get(8).unwrap_or(0);
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            condition_str,
            action_str,
            enabled != 0,
            fire_count as u64,
            row.get::<_, Option<String>>(7)?,
            priority as u32,
        ))
    });
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| {
        let (id, name, trigger, condition_str, action_str, enabled, fire_count, last_fired, priority) =
            r.ok()?;
        let condition = condition_str
            .and_then(|s| serde_json::from_str(&s).ok());
        let action = serde_json::from_str(&action_str).ok()?;
        Some(crate::rules::AutoRule {
            id,
            name,
            trigger,
            condition,
            action,
            enabled,
            fire_count,
            last_fired,
            priority,
        })
    })
    .collect()
}

pub fn db_save_rule(conn: &Connection, rule: &crate::rules::AutoRule) {
    let condition_str = rule
        .condition
        .as_ref()
        .and_then(|c| serde_json::to_string(c).ok());
    let action_str = match serde_json::to_string(&rule.action) {
        Ok(s) => s,
        Err(_) => return,
    };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO rules (id, name, trigger, condition, action, enabled, fire_count, last_fired, priority)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            rule.id,
            rule.name,
            rule.trigger,
            condition_str,
            action_str,
            rule.enabled as i64,
            rule.fire_count as i64,
            rule.last_fired,
            rule.priority as i64,
        ],
    );
}

pub fn db_delete_rule(conn: &Connection, id: &str) -> bool {
    conn.execute("DELETE FROM rules WHERE id = ?1", params![id])
        .map(|n| n > 0)
        .unwrap_or(false)
}

pub fn db_save_rule_history(conn: &Connection, event: &crate::rules::RuleFireEvent) {
    let _ = conn.execute(
        "INSERT INTO rule_history (rule_id, timestamp, trigger, result) VALUES (?1, ?2, ?3, ?4)",
        params![event.rule_id, event.timestamp, event.trigger, event.result],
    );
    // Keep last 100
    let _ = conn.execute(
        "DELETE FROM rule_history WHERE id NOT IN (
            SELECT id FROM rule_history ORDER BY id DESC LIMIT 100
         )",
        [],
    );
}

pub fn db_load_rule_history(conn: &Connection, rule_id: Option<&str>) -> Vec<crate::rules::RuleFireEvent> {
    let (sql, filter_id): (&str, bool) = match rule_id {
        Some(_) => (
            "SELECT rule_id, timestamp, trigger, result FROM rule_history WHERE rule_id = ?1 ORDER BY id DESC LIMIT 100",
            true,
        ),
        None => (
            "SELECT rule_id, timestamp, trigger, result FROM rule_history ORDER BY id DESC LIMIT 100",
            false,
        ),
    };

    if filter_id {
        let id = rule_id.unwrap();
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![id], |row| {
            Ok(crate::rules::RuleFireEvent {
                rule_id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger: row.get(2)?,
                result: row.get(3)?,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    } else {
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |row| {
            Ok(crate::rules::RuleFireEvent {
                rule_id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger: row.get(2)?,
                result: row.get(3)?,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Crons helpers
// ---------------------------------------------------------------------------

pub fn db_load_crons(conn: &Connection) -> Vec<crate::crons::CronJob> {
    let mut stmt = match conn.prepare(
        "SELECT id, name, cron_expr, target, prompt, enabled, last_run, next_run, run_count, created_at FROM cron_jobs",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |row| {
        let enabled: i64 = row.get(5)?;
        let run_count: i64 = row.get(8)?;
        Ok(crate::crons::CronJob {
            id: row.get(0)?,
            name: row.get(1)?,
            cron_expr: row.get(2)?,
            target: row.get(3)?,
            prompt: row.get(4)?,
            enabled: enabled != 0,
            last_run: row.get(6)?,
            next_run: row.get(7)?,
            run_count: run_count as u64,
            created_at: row.get(9).unwrap_or_else(|_| chrono::Utc::now().to_rfc3339()),
        })
    });
    let Ok(rows) = rows else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

pub fn db_save_cron(conn: &Connection, job: &crate::crons::CronJob) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO cron_jobs
         (id, name, cron_expr, target, prompt, enabled, last_run, next_run, run_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            job.id,
            job.name,
            job.cron_expr,
            job.target,
            job.prompt,
            job.enabled as i64,
            job.last_run,
            job.next_run,
            job.run_count as i64,
            job.created_at,
        ],
    );
}

pub fn db_delete_cron(conn: &Connection, id: &str) -> bool {
    conn.execute("DELETE FROM cron_jobs WHERE id = ?1", params![id])
        .map(|n| n > 0)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Migration table helpers
// ---------------------------------------------------------------------------

/// Ensure the _migrations tracking table exists.
fn ensure_migrations_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
}

/// Check whether `version` has already been applied.
fn migration_applied(conn: &Connection, version: u32) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _migrations WHERE version = ?1",
        params![version as i64],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Run `sql` inside a transaction if `version` has not yet been recorded.
/// Records the migration on success; silently skips if already applied.
pub fn run_migration(conn: &Connection, version: u32, sql: &str) {
    if let Err(e) = ensure_migrations_table(conn) {
        log::warn!("run_migration: could not ensure _migrations table: {e}");
        return;
    }
    if migration_applied(conn, version) {
        return;
    }
    match conn.unchecked_transaction() {
        Ok(tx) => {
            if let Err(e) = tx.execute_batch(sql) {
                log::warn!("run_migration v{version}: SQL failed: {e}");
                return;
            }
            let now = chrono::Utc::now().to_rfc3339();
            let _ = tx.execute(
                "INSERT OR IGNORE INTO _migrations (version, applied_at) VALUES (?1, ?2)",
                params![version as i64, now],
            );
            let _ = tx.commit();
            log::info!("run_migration: applied version {version}");
        }
        Err(e) => log::warn!("run_migration v{version}: transaction failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands — database administration
// ---------------------------------------------------------------------------

/// Returns high-level statistics about every user table in the database.
/// Shape: `{ tables: [{name, row_count, size_kb}], total_size_kb, db_path }`.
#[tauri::command]
pub fn get_db_stats(db: tauri::State<Db>) -> serde_json::Value {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());

    // List all user tables (exclude internal SQLite tables and our _migrations).
    let table_names: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .unwrap_or_else(|_| panic!("prepare failed"));
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap_or_else(|_| panic!("query failed"))
            .filter_map(|r| r.ok())
            .collect()
    };

    let mut tables = serde_json::Value::Array(vec![]);
    let arr = tables.as_array_mut().unwrap();

    for name in &table_names {
        let row_count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{name}\""),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        arr.push(serde_json::json!({
            "name": name,
            "row_count": row_count,
        }));
    }

    // page_count * page_size gives us the total database size in bytes.
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap_or(0);
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap_or(4096);
    let total_bytes = page_count * page_size;
    let total_size_kb = total_bytes / 1024;

    serde_json::json!({
        "tables": arr,
        "total_size_kb": total_size_kb,
        "db_path": db_path().to_string_lossy(),
    })
}

/// Run `VACUUM` to reclaim unused space from deleted rows.
#[tauri::command]
pub fn vacuum_db(db: tauri::State<Db>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    conn.execute_batch("VACUUM;").map_err(|e| e.to_string())
}

/// Return the highest migration version that has been applied (0 if none).
#[tauri::command]
pub fn get_db_migration_version(db: tauri::State<Db>) -> u32 {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    if ensure_migrations_table(&conn).is_err() {
        return 0;
    }
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

// ---------------------------------------------------------------------------
// Migration from old JSON files
// ---------------------------------------------------------------------------

pub fn migrate_from_json(db: &Db) {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());

    // Migrate rules.json
    migrate_rules_json(&conn);

    // Migrate rules-history.json
    migrate_rule_history_json(&conn);

    // Migrate crons.json
    migrate_crons_json(&conn);

    // Migrate task-history.json
    migrate_task_history_json(&conn);
}

fn json_is_empty(path: &std::path::Path) -> bool {
    if !path.exists() {
        return true;
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| match v {
            serde_json::Value::Array(a) => a.is_empty(),
            _ => true,
        })
        .unwrap_or(true)
}

fn migrate_rules_json(conn: &Connection) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = home.join(".config/jarvis/rules.json");
    if json_is_empty(&path) {
        return;
    }

    // Check if DB already has rules
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM rules", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        // Already migrated
        let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        return;
    }

    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(rules) = serde_json::from_str::<Vec<crate::rules::AutoRule>>(&data) {
            for rule in &rules {
                db_save_rule(conn, rule);
            }
            log::info!("Migrated {} rules from rules.json to SQLite", rules.len());
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
    }
}

fn migrate_rule_history_json(conn: &Connection) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = home.join(".config/jarvis/rules-history.json");
    if json_is_empty(&path) {
        return;
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM rule_history", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        return;
    }

    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(events) = serde_json::from_str::<Vec<crate::rules::RuleFireEvent>>(&data) {
            for event in &events {
                db_save_rule_history(conn, event);
            }
            log::info!(
                "Migrated {} rule history entries from rules-history.json to SQLite",
                events.len()
            );
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
    }
}

fn migrate_crons_json(conn: &Connection) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = home.join(".config/jarvis/crons.json");
    if json_is_empty(&path) {
        return;
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cron_jobs", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        return;
    }

    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(mut crons) = serde_json::from_str::<Vec<crate::crons::CronJob>>(&data) {
            // Ensure created_at is populated (old records may not have it)
            let now_iso = chrono::Utc::now().to_rfc3339();
            for cron in crons.iter_mut() {
                if cron.created_at.is_empty() {
                    cron.created_at = now_iso.clone();
                }
            }
            for cron in &crons {
                db_save_cron(conn, cron);
            }
            log::info!("Migrated {} crons from crons.json to SQLite", crons.len());
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests (in-memory SQLite)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_tables(&conn).unwrap();
        conn
    }

    #[test]
    fn test_ensure_migrations_table_idempotent() {
        let conn = setup_conn();
        // Calling twice must not fail.
        ensure_migrations_table(&conn).unwrap();
        ensure_migrations_table(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_run_migration_applies_once() {
        let conn = setup_conn();
        run_migration(&conn, 1, "CREATE TABLE IF NOT EXISTS test_v1 (id INTEGER PRIMARY KEY);");
        // Verify table exists.
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test_v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1);
        // Running the same migration again should be a no-op (no error).
        run_migration(&conn, 1, "CREATE TABLE IF NOT EXISTS test_v1 (id INTEGER PRIMARY KEY);");
        let recorded: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations WHERE version=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(recorded, 1, "migration must be recorded exactly once");
    }

    #[test]
    fn test_get_db_migration_version_empty() {
        let conn = setup_conn();
        ensure_migrations_table(&conn).unwrap();
        let version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u32;
        assert_eq!(version, 0);
    }

    #[test]
    fn test_migration_version_increments() {
        let conn = setup_conn();
        run_migration(&conn, 1, "SELECT 1;");
        run_migration(&conn, 2, "SELECT 2;");
        run_migration(&conn, 3, "SELECT 3;");
        let version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u32;
        assert_eq!(version, 3);
    }

    #[test]
    fn test_migration_applied_flag() {
        let conn = setup_conn();
        ensure_migrations_table(&conn).unwrap();
        assert!(!migration_applied(&conn, 42));
        run_migration(&conn, 42, "SELECT 42;");
        assert!(migration_applied(&conn, 42));
    }

    #[test]
    fn test_page_count_query() {
        // Smoke-test the PRAGMA queries used by get_db_stats.
        let conn = setup_conn();
        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap_or(0);
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .unwrap_or(4096);
        assert!(page_count > 0, "page_count should be positive");
        assert!(page_size >= 512, "page_size should be at least 512 bytes");
    }
}

fn migrate_task_history_json(conn: &Connection) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = home.join(".config/jarvis/task-history.json");
    if json_is_empty(&path) {
        return;
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM task_history", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        return;
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OldEntry {
        id: u64,
        target: String,
        prompt: String,
        output: String,
        status: String,
        duration_secs: u64,
    }

    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(entries) = serde_json::from_str::<Vec<OldEntry>>(&data) {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            for entry in &entries {
                let success = entry.status == "success";
                let output_trimmed: String = entry.output.chars().take(5000).collect();
                let _ = conn.execute(
                    "INSERT INTO task_history (task_id, target, prompt, status, output, duration_secs, completed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        entry.id as i64,
                        entry.target,
                        entry.prompt,
                        entry.status,
                        output_trimmed,
                        entry.duration_secs as f64,
                        now_secs,
                    ],
                );
                let _ = success; // suppress unused warning
            }
            log::info!(
                "Migrated {} task history entries from task-history.json to SQLite",
                entries.len()
            );
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
    }
}
