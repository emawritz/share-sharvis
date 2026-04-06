use serde::{Deserialize, Serialize};
use std::str::FromStr;
use cron::Schedule;
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub cron_expr: String,   // e.g. "0 9 * * *"
    pub target: String,      // "atlas", "pixel", "both"
    pub prompt: String,
    pub enabled: bool,
    pub last_run: Option<String>,   // ISO8601
    pub next_run: Option<String>,   // ISO8601
    pub run_count: u64,
    #[serde(default = "default_created_at")]
    pub created_at: String,  // ISO8601
}

fn default_created_at() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn validate_cron_expr(expr: &str) -> Result<(), String> {
    // cron crate expects 6-field (with seconds) or 5-field. Normalize to 6-field by prepending "0 "
    let expr6 = format!("0 {}", expr);
    Schedule::from_str(&expr6).map_err(|e| format!("Invalid cron expression: {}", e))?;
    Ok(())
}

pub fn compute_next_run(expr: &str) -> Option<String> {
    let expr6 = format!("0 {}", expr);
    let schedule = Schedule::from_str(&expr6).ok()?;
    let next = schedule.upcoming(Utc).next()?;
    Some(next.to_rfc3339())
}

/// Called from background thread — check all enabled crons and fire any due ones.
/// Returns list of (cron_id, target, prompt) for jobs that fired.
pub fn check_and_fire(now: chrono::DateTime<Utc>, app: &tauri::AppHandle) -> Vec<(String, String, String)> {
    use tauri::Manager;
    let db = app.state::<crate::db::Db>();
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());

    let mut crons = crate::db::db_load_crons(&conn);
    let mut fired = Vec::new();

    for cron in crons.iter_mut() {
        if !cron.enabled || cron.prompt.is_empty() {
            continue;
        }
        let expr6 = format!("0 {}", cron.cron_expr);
        let Ok(schedule) = Schedule::from_str(&expr6) else { continue };

        // Find the most recent scheduled time before or equal to now
        // We fire if that time is within the last 60 seconds (our check interval)
        let window_start = now - chrono::Duration::seconds(60);
        let due = schedule.after(&window_start).next().map(|t| t <= now).unwrap_or(false);

        // Guard: don't re-fire if already ran within this window
        let already_ran = cron.last_run.as_deref().is_some_and(|lr| {
            chrono::DateTime::parse_from_rfc3339(lr)
                .map(|t| t.with_timezone(&Utc) >= window_start)
                .unwrap_or(false)
        });

        if due && !already_ran {
            cron.last_run = Some(now.to_rfc3339());
            cron.run_count += 1;
            cron.next_run = compute_next_run(&cron.cron_expr);
            fired.push((cron.id.clone(), cron.target.clone(), cron.prompt.clone()));
        }
    }

    // Only persist the crons that actually fired (avoids overwriting concurrent UI edits)
    if !fired.is_empty() {
        let fired_ids: std::collections::HashSet<&str> = fired.iter().map(|(id, _, _)| id.as_str()).collect();
        for cron in crons.iter().filter(|c| fired_ids.contains(c.id.as_str())) {
            crate::db::db_save_cron(&conn, cron);
        }
    }
    fired
}

// --- Helpers ---

/// Build a simple human-readable description for a 5-field cron expression.
fn human_readable(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return expr.to_string();
    }
    let (min, hour, day, month, weekday) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // Weekday names
    fn weekday_name(n: &str) -> &str {
        match n {
            "0" | "7" => "Sunday",
            "1" => "Monday",
            "2" => "Tuesday",
            "3" => "Wednesday",
            "4" => "Thursday",
            "5" => "Friday",
            "6" => "Saturday",
            _ => n,
        }
    }

    // Month names
    fn month_name(n: &str) -> &str {
        match n {
            "1" => "January", "2" => "February", "3" => "March",
            "4" => "April", "5" => "May", "6" => "June",
            "7" => "July", "8" => "August", "9" => "September",
            "10" => "October", "11" => "November", "12" => "December",
            _ => n,
        }
    }

    // Time part
    let time_str = if hour != "*" && min != "*" {
        let h = hour.parse::<u8>().unwrap_or(0);
        let m = min.parse::<u8>().unwrap_or(0);
        format!("at {:02}:{:02} UTC", h, m)
    } else if hour != "*" {
        format!("at hour {} UTC", hour)
    } else {
        "every minute".to_string()
    };

    // Frequency
    match (day, month, weekday) {
        ("*", "*", "*") => format!("Every day {}", time_str),
        ("*", "*", wd) if wd != "*" => {
            // Could be a list or single
            if wd.contains(',') {
                let days: Vec<&str> = wd.split(',').map(weekday_name).collect();
                format!("Every {} {}", days.join(", "), time_str)
            } else {
                format!("Every {} {}", weekday_name(wd), time_str)
            }
        }
        (d, "*", "*") if d != "*" => format!("Day {} of every month {}", d, time_str),
        (d, m, "*") if d != "*" && m != "*" => {
            format!("{} {} {}", month_name(m), d, time_str)
        }
        _ => format!("Custom schedule: {} {} {} {} {} (UTC)", min, hour, day, month, weekday),
    }
}

// --- Tauri commands ---

#[tauri::command]
pub fn get_cron_next_runs(db: tauri::State<'_, crate::db::Db>) -> Vec<serde_json::Value> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let crons = crate::db::db_load_crons(&conn);
    crons.iter().map(|c| {
        let next = compute_next_run(&c.cron_expr);
        serde_json::json!({ "id": c.id, "nextRun": next })
    }).collect()
}

#[tauri::command]
pub fn validate_cron_expr_cmd(expr: String) -> Result<String, String> {
    let expr6 = format!("0 {}", expr.trim());
    Schedule::from_str(&expr6).map_err(|e| format!("Invalid expression: {}", e))?;
    Ok(human_readable(expr.trim()))
}

#[tauri::command]
pub fn get_crons(db: tauri::State<'_, crate::db::Db>) -> Vec<CronJob> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_load_crons(&conn)
}

#[tauri::command]
pub fn save_cron(db: tauri::State<'_, crate::db::Db>, mut job: CronJob) -> Result<CronJob, String> {
    if job.name.trim().is_empty() { return Err("Name is required".into()); }
    if job.prompt.trim().is_empty() { return Err("Prompt is required".into()); }
    if job.prompt.len() > 10_000 { return Err("Prompt too long".into()); }
    validate_cron_expr(&job.cron_expr)?;
    if job.id.is_empty() { job.id = uuid::Uuid::new_v4().to_string(); }
    if job.created_at.is_empty() { job.created_at = chrono::Utc::now().to_rfc3339(); }
    job.next_run = compute_next_run(&job.cron_expr);

    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_save_cron(&conn, &job);
    Ok(job)
}

#[tauri::command]
pub fn delete_cron(db: tauri::State<'_, crate::db::Db>, id: String) -> bool {
    if id.is_empty() { return false; }
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_delete_cron(&conn, &id)
}

#[tauri::command]
pub fn toggle_cron(db: tauri::State<'_, crate::db::Db>, id: String, enabled: bool) -> Result<bool, String> {
    if id.is_empty() { return Err("ID required".into()); }
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let mut crons = crate::db::db_load_crons(&conn);
    if let Some(c) = crons.iter_mut().find(|c| c.id == id) {
        c.enabled = enabled;
        if enabled { c.next_run = compute_next_run(&c.cron_expr); }
        crate::db::db_save_cron(&conn, c);
        Ok(true)
    } else {
        Err(format!("Cron '{}' not found", id))
    }
}
