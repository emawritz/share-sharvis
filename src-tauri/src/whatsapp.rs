//! WhatsApp ↔ JARVIS bridge — HTTP server on :3141
//!
//! wa-bridge (Node.js Baileys) POSTs inbound messages here.
//! This module dispatches tasks directly and POSTs replies back
//! to the bridge on :3142.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

// ---------------------------------------------------------------------------
// Global state — accessible from tasks.rs completion hooks without Tauri state
// ---------------------------------------------------------------------------

static WHATSAPP_STATE: OnceLock<Arc<WhatsappState>> = OnceLock::new();

/// Unix timestamp (seconds) at which the bridge HTTP server was started.
static SERVER_START_SECS: OnceLock<u64> = OnceLock::new();

pub struct WhatsappState {
    /// task_id → JID, for routing completion replies back
    pub pending_reply: Mutex<HashMap<u64, String>>,
}

// ---------------------------------------------------------------------------
// HTTP payload types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessagePayload {
    pub jid: String,
    pub text: String,
}

// AppState must be Clone — axum's .with_state() requires S: Clone.
// Both AppHandle and Arc<WhatsappState> implement Clone.
#[derive(Clone)]
struct AppState {
    wa: Arc<WhatsappState>,
    app: AppHandle,
}

// ---------------------------------------------------------------------------
// Routing parser
// ---------------------------------------------------------------------------

/// Parse "@atlas|@pixel|@ambos" prefix from a message.
/// Returns (target, clean_prompt). Default target is "atlas".
pub fn parse_routing(text: &str) -> (String, String) {
    let text = text.trim();
    for (prefix, target) in &[
        ("@atlas ", "atlas"),
        ("@pixel ", "pixel"),
        ("@ambos ", "both"),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return (target.to_string(), rest.trim().to_string());
        }
    }
    ("atlas".to_string(), text.to_string())
}

fn target_display(target: &str) -> &str {
    match target {
        "pixel" => "PIXEL",
        "both" => "ATLAS+PIXEL",
        _ => "ATLAS",
    }
}

// ---------------------------------------------------------------------------
// Bridge communication
// ---------------------------------------------------------------------------

/// POST a reply to the wa-bridge on :3142 (non-blocking, best-effort).
pub fn post_reply(jid: &str, text: &str) {
    let jid = jid.to_string();
    let text = text.to_string();
    std::thread::spawn(move || {
        let body = serde_json::json!({ "jid": jid, "text": text });
        match ureq::post("http://localhost:3142/reply")
            .set("Content-Type", "application/json")
            .send_string(&serde_json::to_string(&body).unwrap_or_default())
        {
            Ok(_) => {}
            Err(e) => log::warn!("[whatsapp] Bridge unreachable: {}", e),
        }
    });
}

/// Called from tasks.rs when a task finishes (normal, timeout, and unexpected-exit paths).
/// Looks up the JID for the task and sends the result via WhatsApp.
pub fn notify_task_result(task_id: u64, success: bool, output: &str) {
    let state = match WHATSAPP_STATE.get() {
        Some(s) => s.clone(),
        None => return,
    };

    let jid = state
        .pending_reply
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&task_id);

    let jid = match jid {
        Some(j) => j,
        None => return, // task not originated from WhatsApp
    };

    let truncated: String = output.chars().take(1500).collect();
    let msg = if success {
        format!("Tarea completada, Ema.\n{}", truncated)
    } else {
        format!("Lo siento, Ema. La tarea ha fallado.\n{}", truncated)
    };
    post_reply(&jid, &msg);
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn health() -> &'static str {
    "ok"
}

/// GET /status — returns bridge connection status and uptime.
async fn handle_status(_state: State<AppState>) -> Json<serde_json::Value> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let start = SERVER_START_SECS.get().copied().unwrap_or(now_secs);
    let uptime_seconds = now_secs.saturating_sub(start);

    // Probe the wa-bridge on :3142 to determine if it is reachable
    let connected = tokio::task::spawn_blocking(|| {
        ureq::get("http://localhost:3142/health")
            .timeout(std::time::Duration::from_millis(500))
            .call()
            .is_ok()
    })
    .await
    .unwrap_or(false);

    log::info!("[whatsapp] /status polled — connected={} uptime={}s", connected, uptime_seconds);

    Json(serde_json::json!({
        "connected": connected,
        "bridge_url": "http://localhost:3142",
        "uptime_seconds": uptime_seconds,
    }))
}

/// Proxies GET /sessions from the Python voice-agent server on :3144.
async fn handle_sessions(_state: State<AppState>) -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(|| {
        match ureq::get("http://localhost:3144/sessions").call() {
            Ok(resp) => resp.into_string().unwrap_or_default(),
            Err(_) => r#"{"sessions":[]}"#.to_string(),
        }
    })
    .await;

    let json_str = result.unwrap_or_else(|_| r#"{"sessions":[]}"#.to_string());
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .unwrap_or_else(|_| serde_json::json!({"sessions": []}));
    Json(val)
}

/// Trusted direct dispatch (voice agent, no confirmation required).
#[derive(Deserialize)]
struct DispatchPayload {
    target: String,
    prompt: String,
}

#[derive(Serialize)]
struct DispatchResponse {
    task_id: u64,
    target: String,
}

async fn handle_dispatch(
    State(ctx): State<AppState>,
    Json(payload): Json<DispatchPayload>,
) -> (StatusCode, Json<DispatchResponse>) {
    let target = match payload.target.trim() {
        "pixel" => "pixel",
        "both" | "ambos" => "both",
        _ => "atlas",
    };
    let prompt = payload.prompt.trim().to_string();

    let store = ctx.app.state::<crate::tasks::TaskStore>();
    let task = crate::tasks::send_task_internal(&ctx.app, &store, target, &prompt, false);

    (
        StatusCode::OK,
        Json(DispatchResponse { task_id: task.id, target: target.to_string() }),
    )
}

/// Returns live stats for all machines (CPU, RAM, disk, GPU, uptime).
async fn handle_machines(State(ctx): State<AppState>) -> Json<serde_json::Value> {
    let registry = ctx.app.state::<crate::machines::MachineRegistry>();
    let machines_snapshot: Vec<(String, crate::types::Machine)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.iter().filter(|(_, m)| m.enabled).map(|(id, m)| (id.clone(), m.clone())).collect()
    };

    let handles: Vec<_> = machines_snapshot.into_iter().map(|(id, machine)| {
        tokio::task::spawn_blocking(move || {
            let health = crate::machines::check_machine(&machine);
            let stats = crate::machines::get_machine_stats(&machine);
            (id, machine.name.clone(), health, stats)
        })
    }).collect();

    let mut out = serde_json::Map::new();
    for handle in handles {
        if let Ok((id, name, health, stats)) = handle.await {
            out.insert(id, serde_json::json!({
                "name": name,
                "online": health.online,
                "latency_ms": health.latency_ms,
                "cpu": stats.cpu,
                "mem": stats.mem,
                "disk": stats.disk,
                "gpu": stats.gpu,
                "uptime": stats.uptime,
            }));
        }
    }
    Json(serde_json::Value::Object(out))
}

/// Returns running and recently completed tasks (for voice agent status queries).
async fn handle_tasks(State(ctx): State<AppState>) -> Json<serde_json::Value> {
    let store = ctx.app.state::<crate::tasks::TaskStore>();
    let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let cutoff = (now - 3600) * 1000; // last hour — convert to milliseconds to match finished_at
    let visible: Vec<serde_json::Value> = tasks
        .iter()
        .filter(|t| {
            t.status == "running"
                || t.status == "pending"
                || t.finished_at.map(|f| f > cutoff).unwrap_or(false)
        })
        .map(|t| {
            let finished = t.status == "done" || t.status == "timeout" || t.status == "failed";
            serde_json::json!({
                "id": t.id,
                "target": t.target,
                "status": t.status,
                "prompt": t.prompt.chars().take(120).collect::<String>(),
                "output": if finished {
                    t.output.chars().take(3000).collect::<String>()
                } else {
                    String::new()
                },
                "finished_at": t.finished_at,
            })
        })
        .collect();
    Json(serde_json::json!({ "tasks": visible }))
}

async fn handle_message(
    State(ctx): State<AppState>,
    Json(payload): Json<MessagePayload>,
) -> StatusCode {
    let jid = payload.jid.clone();
    let text_lower = payload.text.trim().to_lowercase();

    // Special slash commands
    if text_lower.starts_with("/status") || text_lower.starts_with("/tareas") {
        let store = ctx.app.state::<crate::tasks::TaskStore>();
        let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let running: Vec<_> = tasks.iter().filter(|t| t.status == "running" || t.status == "pending").collect();
        let recent_done: Vec<_> = tasks.iter().filter(|t| t.status == "done" || t.status == "timeout" || t.status == "failed").take(3).collect();

        let mut msg = if running.is_empty() {
            "No hay tareas en ejecución, Ema.\n".to_string()
        } else {
            format!("Tareas activas ({}):\n", running.len())
        };
        for t in &running {
            msg += &format!("• #{} [{}] {}\n", t.id, t.target.to_uppercase(), t.prompt.chars().take(60).collect::<String>());
        }
        if !recent_done.is_empty() {
            msg += "\nÚltimas completadas:\n";
            for t in &recent_done {
                msg += &format!("• #{} {} — {}\n", t.id, t.status, t.prompt.chars().take(60).collect::<String>());
            }
        }
        drop(tasks);
        post_reply(&jid, msg.trim_end());
        return StatusCode::OK;
    }

    if text_lower.starts_with("/cancelar ") {
        let id_str = text_lower.trim_start_matches("/cancelar ").trim();
        if let Ok(task_id) = id_str.parse::<u64>() {
            let store = ctx.app.state::<crate::tasks::TaskStore>();
            let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id && t.status == "running") {
                task.status = "cancelled".to_string();
                drop(tasks);
                post_reply(&jid, &format!("Tarea #{} cancelada, Ema.", task_id));
            } else {
                drop(tasks);
                post_reply(&jid, &format!("No encontré la tarea #{} en ejecución.", task_id));
            }
        } else {
            post_reply(&jid, "Uso: /cancelar <id>");
        }
        return StatusCode::OK;
    }

    if text_lower.starts_with("/ayuda") {
        post_reply(&jid, "Comandos disponibles, Ema:\n/status — tareas activas\n/tareas — igual que /status\n/cancelar <id> — cancelar tarea\n@atlas <tarea> — ejecutar en ATLAS\n@pixel <tarea> — ejecutar en PIXEL\n@ambos <tarea> — ejecutar en ambos");
        return StatusCode::OK;
    }

    // New message — parse routing and dispatch directly (no confirmation)
    let (target, prompt) = parse_routing(&payload.text);
    if prompt.is_empty() {
        post_reply(&jid, "¿En qué puedo ayudarle, Ema? Envíeme una tarea.");
        return StatusCode::OK;
    }

    let display = target_display(&target);

    // Dispatch immediately
    let store = ctx.app.state::<crate::tasks::TaskStore>();
    let dispatched = crate::tasks::send_task_internal(&ctx.app, &store, &target, &prompt, target == "both");

    // Register task_id → jid for completion reply
    ctx.wa
        .pending_reply
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(dispatched.id, jid.clone());

    spawn_progress_updater(dispatched.id, jid.clone(), prompt.clone());
    // Skip ack for URL analysis tasks (wa-bridge already sent its own ack)
    let is_url_analysis = prompt.contains("browser_navigate") || prompt.contains("dev-browser") || prompt.contains("Analiza este");
    if !is_url_analysis {
        post_reply(&jid, &format!("Ejecutando en {}...", display));
    }
    StatusCode::OK
}

/// Cancel a running task by ID (for the voice agent).
#[derive(Deserialize)]
struct CancelPayload {
    task_id: u64,
}

async fn handle_cancel(
    State(ctx): State<AppState>,
    Json(payload): Json<CancelPayload>,
) -> StatusCode {
    let store = ctx.app.state::<crate::tasks::TaskStore>();
    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(task) = tasks.iter_mut().find(|t| t.id == payload.task_id && t.status == "running") {
        task.status = "cancelled".to_string();
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// ---------------------------------------------------------------------------
// Endpoint: GET /tasks/stream  (Server-Sent Events)
// ---------------------------------------------------------------------------

async fn handle_tasks_sse(State(ctx): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let app = ctx.app.clone();
    let stream = stream::unfold((), move |_| {
        let app = app.clone();
        async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let store = app.state::<crate::tasks::TaskStore>();
            let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let cutoff = (now - 300) * 1000; // last 5 minutes in ms
            let data: Vec<serde_json::Value> = tasks.iter()
                .filter(|t| t.status == "running" || t.status == "pending" ||
                        t.finished_at.map(|f| f > cutoff).unwrap_or(false))
                .map(|t| serde_json::json!({
                    "id": t.id,
                    "target": t.target,
                    "status": t.status,
                    "prompt": t.prompt.chars().take(120).collect::<String>(),
                    "output": if t.status == "done" || t.status == "timeout" {
                        t.output.chars().take(2000).collect::<String>()
                    } else { String::new() },
                }))
                .collect();
            drop(tasks);
            let json = serde_json::to_string(&serde_json::json!({"tasks": data})).unwrap_or_default();
            let event = Event::default().data(json);
            Some((Ok(event), ()))
        }
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Endpoint: POST /notify
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NotifyPayload {
    title: String,
    body: String,
}

async fn handle_notify(
    State(_ctx): State<AppState>,
    Json(payload): Json<NotifyPayload>,
) -> StatusCode {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        payload.body.replace('"', "'"),
        payload.title.replace('"', "'")
    );
    std::thread::spawn(move || {
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    });
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Endpoint: POST /image  (image from WhatsApp for analysis)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ImagePayload {
    jid: String,
    base64: String,
    caption: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

async fn handle_image(
    State(_ctx): State<AppState>,
    Json(payload): Json<ImagePayload>,
) -> StatusCode {
    let jid = payload.jid.clone();
    let caption = payload.caption.clone();
    let base64_data = payload.base64.clone();
    let mime_type = payload.mime_type.clone().unwrap_or_else(|| "image/jpeg".to_string());

    tauri::async_runtime::spawn(async move {
        // Try Claude Vision via Python agent server on :3144
        let analysis_result = tokio::task::spawn_blocking(move || {
            let body = serde_json::json!({
                "base64": base64_data,
                "caption": if caption.is_empty() { "Describí qué hay en esta imagen." } else { &caption },
                "mimeType": mime_type,
            });
            match ureq::post("http://localhost:3144/analyze-image")
                .set("Content-Type", "application/json")
                .send_string(&serde_json::to_string(&body).unwrap_or_default())
            {
                Ok(resp) => {
                    let text = resp.into_string().unwrap_or_default();
                    serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|v| v["analysis"].as_str().map(String::from))
                }
                Err(_) => None,
            }
        })
        .await
        .unwrap_or(None);

        if let Some(analysis) = analysis_result {
            post_reply(&jid, &format!("📷 {}", analysis));
        } else {
            // Fallback: Python agent not running
            post_reply(
                &jid,
                "Recibí la imagen, Ema. El agente de voz no está disponible para analizarla ahora.",
            );
        }
    });

    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Endpoint: GET /config/session  and  PATCH /config/session
// ---------------------------------------------------------------------------

async fn handle_get_session(State(_ctx): State<AppState>) -> Json<serde_json::Value> {
    let cfg = crate::config::load_config();
    let s = cfg.session;
    Json(serde_json::json!({
        "id": s.id,
        "rama": s.rama,
        "objetivo": s.objetivo,
    }))
}

#[derive(Deserialize)]
struct SessionPatch {
    objetivo: Option<String>,
    rama: Option<String>,
}

async fn handle_patch_session(
    State(_ctx): State<AppState>,
    Json(payload): Json<SessionPatch>,
) -> StatusCode {
    let mut cfg = crate::config::load_config();
    if let Some(objetivo) = payload.objetivo {
        cfg.session.objetivo = objetivo;
    }
    if let Some(rama) = payload.rama {
        cfg.session.rama = rama;
    }
    match crate::config::save_config(&cfg) {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            log::warn!("[whatsapp] Failed to save session config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// ---------------------------------------------------------------------------
// Endpoint: GET /github/prs
// ---------------------------------------------------------------------------

async fn handle_github_prs(State(ctx): State<AppState>) -> Json<serde_json::Value> {
    let registry = ctx.app.state::<crate::machines::MachineRegistry>();
    // Collect all github repo slugs from all enabled machines
    let repo_slugs: Vec<String> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.values()
            .filter(|m| m.enabled)
            .flat_map(|m| m.repos.iter())
            .filter(|r| !r.github.is_empty())
            .map(|r| r.github.clone())
            .collect()
    };

    let mut all_prs: Vec<serde_json::Value> = Vec::new();
    for slug in repo_slugs {
        let prs = tokio::task::spawn_blocking(move || {
            crate::github::list_prs(slug)
        }).await;
        if let Ok(Ok(prs)) = prs {
            for pr in prs {
                all_prs.push(serde_json::json!({
                    "number": pr.number,
                    "title": pr.title,
                    "state": pr.state,
                    "headRefName": pr.head_ref_name,
                    "createdAt": pr.created_at,
                }));
            }
        }
    }

    Json(serde_json::json!({ "prs": all_prs }))
}

// ---------------------------------------------------------------------------
// Progress updater — sends "still working" messages every 3 minutes
// ---------------------------------------------------------------------------

fn spawn_progress_updater(task_id: u64, jid: String, prompt: String) {
    std::thread::spawn(move || {
        let mut elapsed_mins = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(180)); // 3 minutes
            elapsed_mins += 3;

            // Check if task is still running
            let state = match WHATSAPP_STATE.get() {
                Some(s) => s,
                None => break,
            };

            // If task was removed from pending_reply, it completed — stop updates
            let still_pending = state
                .pending_reply
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&task_id);

            if !still_pending {
                break;
            }

            let short_prompt: String = prompt.chars().take(50).collect();
            post_reply(
                &jid,
                &format!(
                    "Ema, sigo trabajando en tarea #{} ({} min)... \"{}\"",
                    task_id, elapsed_mins, short_prompt
                ),
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Endpoint: GET /crons — list scheduled cron jobs
// ---------------------------------------------------------------------------

async fn handle_list_crons(State(ctx): State<AppState>) -> Json<serde_json::Value> {
    let db = ctx.app.state::<crate::db::Db>();
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let crons = crate::db::db_load_crons(&conn);
    drop(conn);

    let list: Vec<serde_json::Value> = crons
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "cron_expr": c.cron_expr,
                "target": c.target,
                "prompt": c.prompt,
                "enabled": c.enabled,
                "last_run": c.last_run,
                "next_run": c.next_run,
                "run_count": c.run_count,
            })
        })
        .collect();

    Json(serde_json::json!({ "crons": list }))
}

// ---------------------------------------------------------------------------
// Endpoint: POST /crons — create a scheduled cron job
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateCronPayload {
    name: String,
    cron_expr: String,
    prompt: String,
    target: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

async fn handle_create_cron(
    State(ctx): State<AppState>,
    Json(payload): Json<CreateCronPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    use std::str::FromStr;
    let expr6 = format!("0 {}", payload.cron_expr.trim());
    if cron::Schedule::from_str(&expr6).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid cron expression: {}", payload.cron_expr)})),
        );
    }

    let id = format!(
        "voice-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let next_run = crate::crons::compute_next_run(&payload.cron_expr);
    let cron_job = crate::crons::CronJob {
        id: id.clone(),
        name: payload.name.clone(),
        cron_expr: payload.cron_expr.clone(),
        target: payload.target.clone(),
        prompt: payload.prompt.clone(),
        enabled: payload.enabled,
        last_run: None,
        next_run,
        run_count: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let db = ctx.app.state::<crate::db::Db>();
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_save_cron(&conn, &cron_job);
    drop(conn);

    (StatusCode::CREATED, Json(serde_json::json!({"id": id, "name": payload.name})))
}

// ---------------------------------------------------------------------------
// Endpoint: POST /pipeline/run — trigger a pipeline by name
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RunPipelinePayload {
    name: String,
}

async fn handle_run_pipeline(
    State(ctx): State<AppState>,
    Json(payload): Json<RunPipelinePayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    match crate::pipelines::start_pipeline_internal(&ctx.app, &payload.name) {
        Ok(pipeline_id) => (
            StatusCode::OK,
            Json(serde_json::json!({"pipeline_id": pipeline_id, "name": payload.name})),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tauri command: get_whatsapp_status
// ---------------------------------------------------------------------------

/// Returns WhatsApp bridge status for the frontend dashboard.
///
/// Exposed as a Tauri command so the UI can display connection state without
/// going through the HTTP layer.
#[tauri::command]
pub fn get_whatsapp_status() -> serde_json::Value {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let start = SERVER_START_SECS.get().copied().unwrap_or(now_secs);
    let uptime_seconds = now_secs.saturating_sub(start);
    let server_running = WHATSAPP_STATE.get().is_some();

    let pending_reply_count = if let Some(state) = WHATSAPP_STATE.get() {
        state.pending_reply.lock().unwrap_or_else(|e| e.into_inner()).len()
    } else {
        0
    };

    log::info!(
        "[whatsapp] get_whatsapp_status called — running={} uptime={}s pending_reply={}",
        server_running, uptime_seconds, pending_reply_count
    );

    serde_json::json!({
        "server_running": server_running,
        "bridge_url": "http://localhost:3142",
        "uptime_seconds": uptime_seconds,
        "pending_reply_count": pending_reply_count,
    })
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Start the WhatsApp bridge HTTP server on :3141.
/// Called once from lib.rs during app setup.
pub fn start_server(app: AppHandle, shutdown: Arc<AtomicBool>) {
    let state = Arc::new(WhatsappState {
        pending_reply: Mutex::new(HashMap::new()),
    });

    // Record server start time for /status uptime reporting
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = SERVER_START_SECS.set(now_secs);

    // Store globally so notify_task_result can access it from tasks.rs
    if WHATSAPP_STATE.set(state.clone()).is_err() {
        log::warn!("[whatsapp] State already initialized — skipping server start");
        return;
    }

    let ctx = AppState { wa: state, app };

    tauri::async_runtime::spawn(async move {
        let router = Router::new()
            .route("/message", post(handle_message))
            .route("/image", post(handle_image))
            .route("/dispatch", post(handle_dispatch))
            .route("/cancel", post(handle_cancel))
            .route("/tasks", get(handle_tasks))
            .route("/tasks/stream", get(handle_tasks_sse))
            .route("/machines", get(handle_machines))
            .route("/notify", post(handle_notify))
            .route("/config/session", get(handle_get_session).patch(handle_patch_session))
            .route("/github/prs", get(handle_github_prs))
            .route("/health", get(health))
            .route("/status", get(handle_status))
            .route("/sessions", get(handle_sessions))
            .route("/crons", get(handle_list_crons).post(handle_create_cron))
            .route("/pipeline/run", post(handle_run_pipeline))
            .with_state(ctx);

        let listener = match tokio::net::TcpListener::bind("127.0.0.1:3141").await {
            Ok(l) => l,
            Err(e) => {
                log::warn!("[whatsapp] Cannot bind :3141 — bridge disabled: {}", e);
                return;
            }
        };

        log::info!("[whatsapp] Bridge server listening on 127.0.0.1:3141");

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
            .await
            .unwrap_or_else(|e| log::warn!("[whatsapp] Server error: {}", e));
    });
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_routing_atlas_prefix() {
        let (target, prompt) = parse_routing("@atlas hacer git pull");
        assert_eq!(target, "atlas");
        assert_eq!(prompt, "hacer git pull");
    }

    #[test]
    fn test_parse_routing_pixel_prefix() {
        let (target, prompt) = parse_routing("@pixel correr tests");
        assert_eq!(target, "pixel");
        assert_eq!(prompt, "correr tests");
    }

    #[test]
    fn test_parse_routing_ambos_prefix() {
        let (target, prompt) = parse_routing("@ambos git pull");
        assert_eq!(target, "both");
        assert_eq!(prompt, "git pull");
    }

    #[test]
    fn test_parse_routing_no_prefix_defaults_to_atlas() {
        let (target, prompt) = parse_routing("hacer algo");
        assert_eq!(target, "atlas");
        assert_eq!(prompt, "hacer algo");
    }

    #[test]
    fn test_parse_routing_trims_whitespace() {
        let (target, prompt) = parse_routing("  @atlas   hacer algo  ");
        assert_eq!(target, "atlas");
        assert_eq!(prompt, "hacer algo");
    }

    #[test]
    fn test_notify_task_result_no_state_does_not_panic() {
        // WHATSAPP_STATE not initialized in this test binary — must not panic
        notify_task_result(999, true, "some output");
    }

    // --- Slash command parsing ---

    #[test]
    fn test_slash_cancelar_parsing() {
        let text = "/cancelar 42";
        let id_str = text.trim_start_matches("/cancelar ").trim();
        let parsed: Result<u64, _> = id_str.parse();
        assert_eq!(parsed.unwrap(), 42u64);
    }

    #[test]
    fn test_slash_cancelar_invalid_id() {
        let text = "/cancelar abc";
        let id_str = text.trim_start_matches("/cancelar ").trim();
        let parsed: Result<u64, _> = id_str.parse();
        assert!(parsed.is_err());
    }

    #[test]
    fn test_slash_status_recognized() {
        let text = "/status";
        assert!(text.starts_with("/status") || text.starts_with("/tareas"));
    }

    #[test]
    fn test_slash_tareas_recognized() {
        let text = "/tareas";
        assert!(text.starts_with("/status") || text.starts_with("/tareas"));
    }

    // --- Trusted prefix logic ---

    #[test]
    fn test_trusted_prefix_matches_case_insensitive() {
        let trusted_raw = "/deploy,/status,/git";
        let trusted_prefixes: Vec<&str> = trusted_raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let msg = "/Deploy a producción";
        let is_trusted = trusted_prefixes.iter().any(|prefix| {
            msg.trim().to_lowercase().starts_with(&prefix.to_lowercase())
        });
        assert!(is_trusted);
    }

    #[test]
    fn test_trusted_prefix_no_match() {
        let trusted_raw = "/deploy,/status";
        let trusted_prefixes: Vec<&str> = trusted_raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let msg = "hacer git pull";
        let is_trusted = trusted_prefixes.iter().any(|prefix| {
            msg.trim().to_lowercase().starts_with(&prefix.to_lowercase())
        });
        assert!(!is_trusted);
    }

    #[test]
    fn test_trusted_prefix_empty_env() {
        let trusted_raw = "";
        let trusted_prefixes: Vec<&str> = trusted_raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        assert!(trusted_prefixes.is_empty());
    }

    // --- target_display ---

    #[test]
    fn test_target_display_atlas() {
        assert_eq!(target_display("atlas"), "ATLAS");
    }

    #[test]
    fn test_target_display_pixel() {
        assert_eq!(target_display("pixel"), "PIXEL");
    }

    #[test]
    fn test_target_display_both() {
        assert_eq!(target_display("both"), "ATLAS+PIXEL");
    }

    #[test]
    fn test_target_display_unknown_defaults_to_atlas() {
        assert_eq!(target_display("unknown"), "ATLAS");
    }

    // --- default_true helper ---

    #[test]
    fn test_default_true_returns_true() {
        assert!(default_true());
    }

    // --- ImagePayload deserialization ---

    #[test]
    fn test_image_payload_all_fields() {
        let json = r#"{"jid":"12345@s.whatsapp.net","base64":"abc123","caption":"look at this","mimeType":"image/png"}"#;
        let payload: ImagePayload = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(payload.jid, "12345@s.whatsapp.net");
        assert_eq!(payload.base64, "abc123");
        assert_eq!(payload.caption, "look at this");
        assert_eq!(payload.mime_type, Some("image/png".to_string()));
    }

    #[test]
    fn test_image_payload_missing_optional_mime_type() {
        let json = r#"{"jid":"12345@s.whatsapp.net","base64":"data","caption":""}"#;
        let payload: ImagePayload = serde_json::from_str(json).expect("should deserialize without mimeType");
        assert_eq!(payload.jid, "12345@s.whatsapp.net");
        assert_eq!(payload.base64, "data");
        assert_eq!(payload.caption, "");
        assert!(payload.mime_type.is_none());
    }

    #[test]
    fn test_image_payload_default_mime_fallback() {
        // Simulate the handler logic: unwrap_or_else for missing mime_type
        let json = r#"{"jid":"abc@g.us","base64":"xyz","caption":"foto"}"#;
        let payload: ImagePayload = serde_json::from_str(json).unwrap();
        let effective_mime = payload.mime_type.unwrap_or_else(|| "image/jpeg".to_string());
        assert_eq!(effective_mime, "image/jpeg");
    }

    #[test]
    fn test_image_payload_explicit_mime_not_replaced() {
        let json = r#"{"jid":"abc@g.us","base64":"xyz","caption":"foto","mimeType":"image/gif"}"#;
        let payload: ImagePayload = serde_json::from_str(json).unwrap();
        let effective_mime = payload.mime_type.unwrap_or_else(|| "image/jpeg".to_string());
        assert_eq!(effective_mime, "image/gif");
    }

    // --- GET /crons (handle_list_crons) via db_load_crons with in-memory DB ---

    fn make_in_memory_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cron_jobs (
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
            );",
        )
        .expect("create cron_jobs table");
        conn
    }

    #[test]
    fn test_list_crons_empty_returns_empty_vec() {
        let conn = make_in_memory_db();
        let crons = crate::db::db_load_crons(&conn);
        assert!(crons.is_empty());
        // Simulate handle_list_crons serialization logic
        let list: Vec<serde_json::Value> = crons
            .iter()
            .map(|c| serde_json::json!({"id": c.id, "name": c.name}))
            .collect();
        let out = serde_json::json!({ "crons": list });
        assert_eq!(out["crons"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_list_crons_populated_serializes_all_fields() {
        let conn = make_in_memory_db();
        // Insert a cron job directly
        conn.execute(
            "INSERT INTO cron_jobs (id, name, cron_expr, target, prompt, enabled, last_run, next_run, run_count, created_at)
             VALUES ('job-1', 'Daily backup', '0 9 * * *', 'atlas', 'run backup script', 1, NULL, '2026-03-17T09:00:00Z', 0, '2026-03-16T00:00:00Z')",
            [],
        )
        .expect("insert cron");

        let crons = crate::db::db_load_crons(&conn);
        assert_eq!(crons.len(), 1);

        let c = &crons[0];
        assert_eq!(c.id, "job-1");
        assert_eq!(c.name, "Daily backup");
        assert_eq!(c.cron_expr, "0 9 * * *");
        assert_eq!(c.target, "atlas");
        assert_eq!(c.prompt, "run backup script");
        assert!(c.enabled);
        assert_eq!(c.last_run, None);
        assert_eq!(c.next_run, Some("2026-03-17T09:00:00Z".to_string()));
        assert_eq!(c.run_count, 0);

        // Simulate the full JSON serialization from handle_list_crons
        let list: Vec<serde_json::Value> = crons
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "name": c.name,
                    "cron_expr": c.cron_expr,
                    "target": c.target,
                    "prompt": c.prompt,
                    "enabled": c.enabled,
                    "last_run": c.last_run,
                    "next_run": c.next_run,
                    "run_count": c.run_count,
                })
            })
            .collect();
        let out = serde_json::json!({ "crons": list });
        let arr = out["crons"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "job-1");
        assert_eq!(arr[0]["name"], "Daily backup");
        assert_eq!(arr[0]["cron_expr"], "0 9 * * *");
        assert_eq!(arr[0]["target"], "atlas");
        assert_eq!(arr[0]["prompt"], "run backup script");
        assert_eq!(arr[0]["enabled"], true);
        assert!(arr[0]["last_run"].is_null());
        assert_eq!(arr[0]["next_run"], "2026-03-17T09:00:00Z");
        assert_eq!(arr[0]["run_count"], 0);
    }

    #[test]
    fn test_list_crons_multiple_jobs_all_returned() {
        let conn = make_in_memory_db();
        for i in 1..=3u32 {
            conn.execute(
                &format!(
                    "INSERT INTO cron_jobs (id, name, cron_expr, target, prompt, enabled, last_run, next_run, run_count, created_at)
                     VALUES ('job-{}', 'Job {}', '0 {} * * *', 'atlas', 'prompt {}', 1, NULL, NULL, {}, '2026-03-16T00:00:00Z')",
                    i, i, i, i, i * 2
                ),
                [],
            )
            .expect("insert");
        }

        let crons = crate::db::db_load_crons(&conn);
        assert_eq!(crons.len(), 3);
        // run_counts should be 2, 4, 6
        let run_counts: Vec<u64> = crons.iter().map(|c| c.run_count).collect();
        assert!(run_counts.contains(&2));
        assert!(run_counts.contains(&4));
        assert!(run_counts.contains(&6));
    }

    #[test]
    fn test_list_crons_disabled_job_returned() {
        let conn = make_in_memory_db();
        conn.execute(
            "INSERT INTO cron_jobs (id, name, cron_expr, target, prompt, enabled, last_run, next_run, run_count, created_at)
             VALUES ('disabled-job', 'Paused', '0 1 * * *', 'pixel', 'noop', 0, NULL, NULL, 0, '2026-03-16T00:00:00Z')",
            [],
        )
        .unwrap();

        let crons = crate::db::db_load_crons(&conn);
        assert_eq!(crons.len(), 1);
        assert!(!crons[0].enabled);
    }

    // --- Slash command: /ayuda ---

    #[test]
    fn test_slash_ayuda_recognized() {
        let text = "/ayuda";
        assert!(text.to_lowercase().starts_with("/ayuda"));
    }

    #[test]
    fn test_slash_ayuda_with_extra_text() {
        // Handler uses starts_with, so trailing text is fine
        let text = "/ayuda por favor";
        assert!(text.to_lowercase().starts_with("/ayuda"));
    }

    // --- Cron expression validation ---

    #[test]
    fn test_cron_expr_valid() {
        use std::str::FromStr;
        let valid_exprs = ["0 9 * * *", "*/30 * * * *", "0 8 * * 1"];
        for expr in &valid_exprs {
            let expr6 = format!("0 {}", expr);
            assert!(cron::Schedule::from_str(&expr6).is_ok(), "Expected valid: {}", expr);
        }
    }

    #[test]
    fn test_cron_expr_invalid_does_not_panic() {
        use std::str::FromStr;
        let invalid_exprs = ["not-a-cron", "* * *"];
        for expr in &invalid_exprs {
            let expr6 = format!("0 {}", expr);
            let _ = cron::Schedule::from_str(&expr6); // must not panic
        }
    }
}
