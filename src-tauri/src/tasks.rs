use std::collections::HashMap;
use std::fs;
use std::io::Write as IoWrite;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::Manager;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::types::{shell_escape, ConversationEntry, Config, PersistedState, Task, TaskChainStep, TaskGraph};

const MAX_HISTORY: usize = 10;
const POLL_TIMEOUT_SECS: u64 = 30 * 60; // 30 minutes max per task

// ---------------------------------------------------------------------------
// App State
// ---------------------------------------------------------------------------

pub struct TaskStore {
    pub tasks: Mutex<Vec<Task>>,
    pub task_id_counter: Mutex<u64>,
    pub conversation_history: Mutex<HashMap<String, Vec<ConversationEntry>>>,
    pub config: Mutex<Config>,
    pub state_file: Mutex<String>,
}

impl TaskStore {
    pub fn new(state_file: String) -> Self {
        let store = Self {
            tasks: Mutex::new(Vec::new()),
            task_id_counter: Mutex::new(0),
            conversation_history: Mutex::new(HashMap::new()),
            config: Mutex::new(Config::default()),
            state_file: Mutex::new(state_file.clone()),
        };
        store.load_state();
        store.reconcile_running_tasks();
        store
    }

    fn load_state(&self) {
        let path = self.state_file.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<PersistedState>(&data) {
                *self.tasks.lock().unwrap_or_else(|e| e.into_inner()) = state.tasks;
                *self.task_id_counter.lock().unwrap_or_else(|e| e.into_inner()) = state.task_id_counter;
                *self.conversation_history.lock().unwrap_or_else(|e| e.into_inner()) = state.conversation_history;
                *self.config.lock().unwrap_or_else(|e| e.into_inner()) = state.config;
            }
        }
    }

    pub fn save_state(&self) {
        let path = self.state_file.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let state = PersistedState {
            tasks: self.tasks.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            task_id_counter: *self.task_id_counter.lock().unwrap_or_else(|e| e.into_inner()),
            conversation_history: self.conversation_history.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            config: self.config.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            // Ensure parent directory exists
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, json);
        }
    }

    /// On startup, reconcile tasks stuck as "running" from a previous session.
    /// If the .done file exists, read output and mark complete. Otherwise mark as done (orphaned).
    fn reconcile_running_tasks(&self) {
        let mut changed = false;
        {
            let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            for t in tasks.iter_mut() {
                if t.status != "running" {
                    continue;
                }
                let done_file = format!("/tmp/jarvis-task-{}.done", t.id);
                let output_file = format!("/tmp/jarvis-task-{}.out", t.id);
                if std::path::Path::new(&done_file).exists() {
                    // Task completed but polling thread died — recover output
                    t.output = fs::read_to_string(&output_file).unwrap_or_default();
                    t.status = "done".to_string();
                    t.finished_at = Some(now_millis());
                    let _ = fs::remove_file(&output_file);
                    let _ = fs::remove_file(&done_file);
                    changed = true;
                } else {
                    // No done file and no process — mark as orphaned
                    t.status = "done".to_string();
                    t.finished_at = Some(now_millis());
                    t.output = "[Tarea huerfana - proceso terminado sin resultado]".to_string();
                    changed = true;
                }
            }
        }
        if changed {
            self.save_state();
        }
    }

    fn next_id(&self) -> u64 {
        let mut counter = self.task_id_counter.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        *counter
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn build_context_prompt(store: &TaskStore, target: &str, prompt: &str, orchestrate: bool) -> String {
    // Inject team context (memories + unread messages)
    let team_context = crate::messages::get_team_context_for(target);

    let history = store.conversation_history.lock().unwrap_or_else(|e| e.into_inner());
    let entries = history.get(target);
    let mut full = String::new();

    if !team_context.is_empty() {
        full.push_str(&team_context);
        full.push_str("---\n");
    }

    if let Some(entries) = entries {
        if !entries.is_empty() {
            full.push_str("CONTEXTO — Historial de tareas anteriores en esta sesion:\n");
            for h in entries {
                full.push_str(&format!(
                    "\n[Tarea #{}] Pedido: {}\n",
                    h.id,
                    h.prompt.chars().take(300).collect::<String>()
                ));
                if !h.output.is_empty() {
                    let mut out = h.output.clone();
                    if let Some(idx) = out.find("===PIXEL-TASK===") {
                        out.truncate(idx);
                        out = out.trim().to_string();
                    }
                    full.push_str(&format!(
                        "Resultado: {}\n",
                        out.chars().take(800).collect::<String>()
                    ));
                }
            }
            full.push_str("\n---\nNUEVA TAREA:\n");
        }
    }

    full.push_str(prompt);

    if orchestrate {
        full.push_str("\n\n---\nIMPORTANTE: Al final de tu respuesta, escribi una seccion que empiece con la linea exacta \"===PIXEL-TASK===\" seguida del prompt/instrucciones que le queres mandar a tu companero PIXEL (frontend Angular). PIXEL va a recibir ESE texto como su tarea. Describile que necesitas del frontend para que funcione con lo que implementaste en el backend.");
    }

    full
}

fn resolve_smart_target(store: &TaskStore, prompt: &str) -> (String, String) {
    let prompt_lower = prompt.to_lowercase();

    let cfg = crate::config::load_config();
    let default_atlas: Vec<String> = vec![
        "backend", "api", "node", "sql", "rust", "server", "database", "migration", "endpoint",
        "express", "sequelize", "afip",
    ].into_iter().map(String::from).collect();
    let default_pixel: Vec<String> = vec![
        "frontend", "angular", "component", "css", "tailwind", "html", "svelte", "ui", "ux",
        "style", "template", "scss",
    ].into_iter().map(String::from).collect();

    let atlas_kw = cfg.session.keywords_atlas.unwrap_or(default_atlas);
    let pixel_kw = cfg.session.keywords_pixel.unwrap_or(default_pixel);

    for kw in &atlas_kw {
        if prompt_lower.contains(kw) {
            return ("atlas".to_string(), format!("keyword: {}", kw));
        }
    }
    for kw in &pixel_kw {
        if prompt_lower.contains(kw) {
            return ("pixel".to_string(), format!("keyword: {}", kw));
        }
    }

    let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
    let atlas_busy = tasks.iter().any(|t| t.target == "atlas" && t.status == "running");
    let pixel_busy = tasks.iter().any(|t| t.target == "pixel" && t.status == "running");
    drop(tasks);

    match (atlas_busy, pixel_busy) {
        (true, false) => ("pixel".to_string(), "menor carga".to_string()),
        (false, true) => ("atlas".to_string(), "menor carga".to_string()),
        _ => {
            let counter = *store.task_id_counter.lock().unwrap_or_else(|e| e.into_inner());
            if counter % 2 == 0 {
                ("atlas".to_string(), "round-robin".to_string())
            } else {
                ("pixel".to_string(), "round-robin".to_string())
            }
        }
    }
}

fn check_repo_conflict(app: &AppHandle, target: &str, store: &TaskStore) -> Option<String> {
    let registry = app.state::<crate::machines::MachineRegistry>();
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());

    let target_repos: Vec<String> = machines.get(target)
        .map(|m| m.repos.iter().map(|r| r.name.clone()).collect())
        .unwrap_or_default();
    if target_repos.is_empty() {
        drop(machines);
        return None;
    }

    let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
    let running_others: Vec<&crate::types::Task> = tasks.iter()
        .filter(|t| t.status == "running" && t.target != target)
        .collect();

    for task in running_others {
        let other_repos: Vec<String> = machines.get(&task.target)
            .map(|m| m.repos.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default();

        for repo in &target_repos {
            if other_repos.contains(repo) {
                let msg = format!(
                    "{} y {} trabajan en {}",
                    target.to_uppercase(),
                    task.target.to_uppercase(),
                    repo
                );
                drop(tasks);
                drop(machines);
                return Some(msg);
            }
        }
    }
    drop(tasks);
    drop(machines);
    None
}

pub fn send_task_internal(app: &AppHandle, store: &TaskStore, target: &str, prompt: &str, orchestrate: bool) -> Task {
    send_task_internal_with_repo(app, store, target, prompt, orchestrate, None, Vec::new(), "on_success".to_string())
}

#[allow(clippy::too_many_arguments)]
pub fn send_task_internal_with_repo(app: &AppHandle, store: &TaskStore, target: &str, prompt: &str, orchestrate: bool, repo: Option<&str>, depends_on: Vec<u64>, run_condition: String) -> Task {
    send_task_internal_with_deps_and_repo(app, store, target, prompt, orchestrate, repo, depends_on, run_condition)
}

pub fn send_task_internal_with_deps(app: &AppHandle, store: &TaskStore, target: &str, prompt: &str, orchestrate: bool, depends_on: Vec<u64>, run_condition: String) -> Task {
    send_task_internal_with_deps_and_repo(app, store, target, prompt, orchestrate, None, depends_on, run_condition)
}

#[allow(clippy::too_many_arguments)]
pub fn send_task_internal_with_deps_and_repo(app: &AppHandle, store: &TaskStore, target: &str, prompt: &str, orchestrate: bool, repo: Option<&str>, depends_on: Vec<u64>, run_condition: String) -> Task {
    let id = store.next_id();
    let task = Task {
        id,
        target: target.to_string(),
        prompt: prompt.to_string(),
        status: if depends_on.is_empty() { "running".to_string() } else { "pending".to_string() },
        orchestrate,
        started_at: if depends_on.is_empty() { Some(now_millis()) } else { None },
        finished_at: None,
        output: String::new(),
        pixel_task_id: None,
        depends_on: depends_on.clone(),
        run_condition,
    };

    log::info!("Task #{} dispatched to '{}' (deps={:?})", id, target, depends_on);

    {
        let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.push(task.clone());
    }
    store.save_state();

    // If task has dependencies, don't start it now — it will be dispatched when deps complete
    if !depends_on.is_empty() {
        return task;
    }

    // Conflict detection
    if let Some(conflict) = check_repo_conflict(app, target, store) {
        let _ = app.emit("repo-conflict", serde_json::json!({ "message": conflict }));
        crate::notifications::send_native(app, "JARVIS - Conflicto", &conflict);
        let _ = crate::messages::send_agent_message(
            "jarvis".into(), "all".into(), "conflict".into(), conflict, vec![],
        );
    }

    // Resolve repo_path and host from MachineRegistry config
    let registry = app.state::<crate::machines::MachineRegistry>();
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    let machine = machines.get(target);
    let repo_path = machine
        .and_then(|m| {
            if let Some(repo_name) = repo {
                // Look up by name; fall back to first if not found
                m.repos.iter().find(|r| r.name == repo_name).map(|r| r.path.clone())
                    .or_else(|| m.repos.first().map(|r| r.path.clone()))
            } else {
                m.repos.first().map(|r| r.path.clone())
            }
        })
        .or_else(|| machine.and_then(|m| m.repo_path.clone()))
        .unwrap_or_else(|| {
            crate::app_logs::log_warn(app, format!("task {id}: machine '{target}' has no repos configured, using home dir fallback"));
            "~".to_string()
        });
    let host = machine.map(|m| m.host.clone()).unwrap_or_else(|| target.to_string());
    drop(machines);

    let actual_prompt = build_context_prompt(store, target, prompt, orchestrate);
    let output_file = format!("/tmp/jarvis-task-{}.out", id);
    let done_file = format!("/tmp/jarvis-task-{}.done", id);

    let target_owned = target.to_string();
    let output_file_clone = output_file.clone();
    let done_file_clone = done_file.clone();
    let app_clone = app.clone();

    // Spawn the claude process
    // macOS .app bundles inherit minimal PATH — ensure claude is findable
    let extra_path = format!(
        "{}/.local/bin:{}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin",
        dirs::home_dir().unwrap_or_default().display(),
        dirs::home_dir().unwrap_or_default().display(),
    );
    let mut proc: Option<std::process::Child> = if host == "local" {
        let bash_cmd = format!(
            "export PATH=\"{extra_path}:$PATH\"; cd {repo_path} 2>/dev/null || {{ printf 'ERROR: directorio no encontrado: {repo_path}\\n' > {output_file}; touch {done_file}; exit 0; }}; unset CLAUDECODE; claude -p \"$(cat)\" --output-format text > {output_file} 2>&1; _ec=$?; [ $_ec -ne 0 ] && [ ! -s {output_file} ] && printf '[exit %d] claude terminó sin output (API error, rate limit, o sesion invalida)\\n' $_ec > {output_file}; touch {done_file}",
            extra_path = extra_path,
            repo_path = shell_escape(&repo_path),
            output_file = output_file,
            done_file = done_file
        );
        let mut child = Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        if let Some(ref mut c) = child {
            if let Some(ref mut stdin) = c.stdin {
                let _ = stdin.write_all(actual_prompt.as_bytes());
            }
        }
        child
    } else {
        let bash_cmd = format!(
            "export PATH=\"{extra_path}:$PATH\"; cat | ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=20 {host} \"unset CLAUDECODE; cd {repo_path} 2>&1 || cd ~; claude -p --output-format text 2>&1\" > {output_file} 2>&1; _ec=$?; [ $_ec -ne 0 ] && [ ! -s {output_file} ] && printf '[SSH exit %d] conexion o ejecucion fallo en {host}\\n' $_ec > {output_file}; touch {done_file}",
            extra_path = extra_path,
            host = shell_escape(&host),
            repo_path = shell_escape(&repo_path),
            output_file = output_file,
            done_file = done_file
        );
        let mut child = Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        if let Some(ref mut c) = child {
            if let Some(ref mut stdin) = c.stdin {
                let _ = stdin.write_all(actual_prompt.as_bytes());
            }
        }
        child
    };

    // Close stdin to signal EOF to the child process
    drop(proc.as_mut().and_then(|c| c.stdin.take()));

    let _ = app.emit("task-started", serde_json::json!({ "id": id, "target": target }));

    // Poll for completion in a background thread
    // We need a reference to TaskStore but can't send &TaskStore across threads.
    // Instead, pass the state_file path and task info, and use AppHandle's state.
    let task_id = id;
    let task_target = target_owned.clone();
    let task_prompt = prompt.to_string();
    let task_orchestrate = orchestrate;
    let task_started = task.started_at;

    thread::spawn(move || {
        let mut child_proc = proc;
        let poll_start = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_secs(POLL_TIMEOUT_SECS);

        loop {
            thread::sleep(std::time::Duration::from_secs(3));

            // Check for timeout
            if poll_start.elapsed() >= timeout_duration {
                // Kill the child process
                if let Some(ref mut child) = child_proc {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                let output = fs::read_to_string(&output_file_clone).unwrap_or_default();
                let _ = fs::remove_file(&output_file_clone);
                let _ = fs::remove_file(&done_file_clone);

                let store = app_clone.state::<TaskStore>();
                {
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.status = "timeout".to_string();
                        t.finished_at = Some(now_millis());
                        t.output = if output.is_empty() {
                            "[Timeout: tarea excedió 30 minutos]".to_string()
                        } else {
                            format!("{}\n\n[Timeout: tarea excedió 30 minutos]", output)
                        };
                    }
                }
                store.save_state();
                crate::app_logs::log_warn(&app_clone, format!("Task #{} on '{}' timed out after 30 min", task_id, task_target));
                let _ = app_clone.emit("task-done", serde_json::json!({
                    "id": task_id,
                    "target": task_target,
                    "output": "[Timeout: tarea excedió 30 minutos]"
                }));
                crate::notifications::send_native(
                    &app_clone,
                    &format!("JARVIS - {}", task_target.to_uppercase()),
                    "Timeout: tarea excedió 30 minutos",
                );
                crate::whatsapp::notify_task_result(task_id, false, "[Timeout: tarea excedió 30 minutos]");
                break;
            }

            if std::path::Path::new(&done_file_clone).exists() {
                // Kill and reap the child process to avoid zombies
                if let Some(ref mut child) = child_proc {
                    let _ = child.kill();
                    let _ = child.wait();
                }

                let output = fs::read_to_string(&output_file_clone).unwrap_or_default();
                let _ = fs::remove_file(&output_file_clone);
                let _ = fs::remove_file(&done_file_clone);

                log::info!("Task #{} on '{}' completed ({} bytes output)", task_id, task_target, output.len());

                // Update task via app state
                let store = app_clone.state::<TaskStore>();
                {
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.status = "done".to_string();
                        t.finished_at = Some(now_millis());
                        t.output = output.clone();
                    }
                }

                // Save conversation history
                {
                    let mut history = store.conversation_history.lock().unwrap_or_else(|e| e.into_inner());
                    let entries = history
                        .entry(task_target.clone())
                        .or_insert_with(Vec::new);
                    entries.push(ConversationEntry {
                        id: task_id,
                        prompt: task_prompt.clone(),
                        output: output.clone(),
                    });
                    if entries.len() > MAX_HISTORY {
                        entries.remove(0);
                    }
                }

                store.save_state();

                // Extract [MEMORY] lines from output
                crate::messages::extract_memories_from_output(&output, &task_target);

                // Evaluate automation rules — use the same heuristic as dispatch_dependents
                let success = task_succeeded(&output);
                let trigger = if success { "on_task_complete" } else { "on_task_fail" };
                let rule_context = crate::rules::RuleContext {
                    target: task_target.clone(),
                    output: output.chars().take(2000).collect(),
                };
                let rule_actions = crate::rules::evaluate_rules(trigger, &rule_context, &app_clone);
                if !rule_actions.is_empty() {
                    crate::rules::execute_rule_actions(&app_clone, rule_actions);
                }

                // Webhook notification
                crate::webhooks::notify_task_complete(task_id, &task_target, success, &output);

                // Native notification
                let notif_title = format!("JARVIS - {}", task_target.to_uppercase());
                let notif_body = if success { "Tarea completada".to_string() } else { format!("Tarea fallida: {}", output.chars().take(80).collect::<String>()) };
                crate::notifications::send_native(&app_clone, &notif_title, &notif_body);
                // WhatsApp notification
                crate::whatsapp::notify_task_result(task_id, success, &output);

                // Save to task history
                let duration = task_started.map(|s| (now_millis() - s).max(0) as u64 / 1000).unwrap_or(0);
                crate::task_history::save_to_history(&app_clone, task_id, &task_target, &task_prompt, &output, success, duration);

                let output_preview: String = output.chars().take(2000).collect();
                let _ = app_clone.emit(
                    "task-done",
                    serde_json::json!({
                        "id": task_id,
                        "target": task_target,
                        "output": output_preview
                    }),
                );

                // Orchestrate: extract PIXEL task
                if task_orchestrate {
                    if let Some(idx) = output.find("===PIXEL-TASK===") {
                        let pixel_prompt = output[idx + "===PIXEL-TASK===".len()..].trim();
                        if !pixel_prompt.is_empty() {
                            let pixel_task = send_task_internal(
                                &app_clone,
                                &store,
                                "pixel",
                                pixel_prompt,
                                false,
                            );
                            let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                                t.pixel_task_id = Some(pixel_task.id);
                            }
                            store.save_state();
                        }
                    }
                }

                // Dispatch dependent tasks (task chain)
                dispatch_dependents(&app_clone, &store, task_id, &output);

                break;
            } else if std::path::Path::new(&output_file_clone).exists() {
                // Update partial output
                if let Ok(partial) = fs::read_to_string(&output_file_clone) {
                    let store = app_clone.state::<TaskStore>();
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.output = partial;
                    }
                }
            }

            // Fallback: check if process died without creating .done file
            if let Some(ref mut child) = child_proc {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Process exited — wait briefly for filesystem sync
                        thread::sleep(std::time::Duration::from_millis(500));
                        if !std::path::Path::new(&done_file_clone).exists() {
                            // Reap the child to avoid zombie
                            let _ = child.wait();
                            // Process died without completing normally
                            let output = fs::read_to_string(&output_file_clone).unwrap_or_default();
                            let _ = fs::remove_file(&output_file_clone);
                            let store = app_clone.state::<TaskStore>();
                            {
                                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                                    t.status = "done".to_string();
                                    t.finished_at = Some(now_millis());
                                    t.output = if output.is_empty() {
                                        format!("[Proceso terminó con código: {}]", status)
                                    } else {
                                        output
                                    };
                                }
                            }
                            store.save_state();
                            let _ = app_clone.emit("task-done", serde_json::json!({
                                "id": task_id,
                                "target": task_target,
                                "output": "[Proceso terminó inesperadamente]"
                            }));
                            crate::notifications::send_native(
                                &app_clone,
                                &format!("JARVIS - {}", task_target.to_uppercase()),
                                "Proceso terminó inesperadamente",
                            );
                            crate::whatsapp::notify_task_result(task_id, false, "[Proceso terminó inesperadamente]");
                            break;
                        }
                    }
                    Ok(None) => {} // Still running
                    Err(_) => {}   // Can't check, continue polling .done file
                }
            }
        }
    });

    task
}

// ---------------------------------------------------------------------------
// Task chain dependency dispatch
// ---------------------------------------------------------------------------

fn task_succeeded(output: &str) -> bool {
    if output.trim().is_empty() {
        return false;
    }
    let lower = output.to_lowercase();
    !lower.contains("error:") && !lower.contains("fatal:") && !lower.contains("panicked at")
}

fn dispatch_dependents(app: &AppHandle, store: &TaskStore, completed_id: u64, output: &str) {
    let success = task_succeeded(output);

    // Find pending tasks that depend on completed_id
    let pending: Vec<Task> = {
        let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.iter()
            .filter(|t| t.status == "pending" && t.depends_on.contains(&completed_id))
            .cloned()
            .collect()
    };

    for pending_task in pending {
        // Check if ALL dependencies are done
        let all_deps_done = {
            let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
            pending_task.depends_on.iter().all(|dep_id| {
                tasks.iter().any(|t| t.id == *dep_id && t.status == "done")
            })
        };

        if !all_deps_done {
            continue;
        }

        // Evaluate run_condition
        let should_run = match pending_task.run_condition.as_str() {
            "always" => true,
            "on_failure" => !success,
            _ => success, // "on_success" default
        };

        if should_run {
            // Activate the pending task: update status, set started_at, and spawn the process
            {
                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(t) = tasks.iter_mut().find(|t| t.id == pending_task.id) {
                    t.status = "running".to_string();
                    t.started_at = Some(now_millis());
                }
            }
            store.save_state();

            // Now spawn the actual work for this task
            spawn_task_process(app, store, &pending_task);
        } else {
            // Skip this task
            {
                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(t) = tasks.iter_mut().find(|t| t.id == pending_task.id) {
                    t.status = "done".to_string();
                    t.finished_at = Some(now_millis());
                    t.output = format!("Omitida: condicion '{}' no cumplida", pending_task.run_condition);
                }
            }
            store.save_state();
            let _ = app.emit("task-done", serde_json::json!({
                "id": pending_task.id,
                "target": pending_task.target,
                "output": format!("Omitida: condicion '{}' no cumplida", pending_task.run_condition)
            }));
            // Recursively dispatch any tasks depending on this skipped one
            dispatch_dependents(app, store, pending_task.id, &format!("Omitida: condicion '{}' no cumplida", pending_task.run_condition));
        }
    }
}

fn spawn_task_process(app: &AppHandle, store: &TaskStore, task: &Task) {
    let registry = app.state::<crate::machines::MachineRegistry>();
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    let machine = machines.get(&task.target);
    let repo_path = machine
        .and_then(|m| m.repos.first().map(|r| r.path.clone()))
        .or_else(|| machine.and_then(|m| m.repo_path.clone()))
        .unwrap_or_else(|| {
            crate::app_logs::log_warn(app, format!("task {}: machine '{}' has no repos configured, using home dir fallback", task.id, task.target));
            "~".to_string()
        });
    let host = machine.map(|m| m.host.clone()).unwrap_or_else(|| task.target.clone());
    drop(machines);

    let actual_prompt = build_context_prompt(store, &task.target, &task.prompt, task.orchestrate);
    let output_file = format!("/tmp/jarvis-task-{}.out", task.id);
    let done_file = format!("/tmp/jarvis-task-{}.done", task.id);

    let extra_path = format!(
        "{}/.local/bin:{}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin",
        dirs::home_dir().unwrap_or_default().display(),
        dirs::home_dir().unwrap_or_default().display(),
    );
    let mut proc: Option<std::process::Child> = if host == "local" {
        let bash_cmd = format!(
            "export PATH=\"{extra_path}:$PATH\"; cd {repo_path} 2>/dev/null || {{ printf 'ERROR: directorio no encontrado: {repo_path}\\n' > {output_file}; touch {done_file}; exit 0; }}; unset CLAUDECODE; claude -p \"$(cat)\" --output-format text > {output_file} 2>&1; _ec=$?; [ $_ec -ne 0 ] && [ ! -s {output_file} ] && printf '[exit %d] claude terminó sin output\\n' $_ec > {output_file}; touch {done_file}",
            extra_path = extra_path, repo_path = shell_escape(&repo_path), output_file = output_file, done_file = done_file
        );
        Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    } else {
        let bash_cmd = format!(
            "export PATH=\"{extra_path}:$PATH\"; cat | ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=20 {host} \"unset CLAUDECODE; cd {repo_path} 2>&1 || cd ~; claude -p --output-format text 2>&1\" > {output_file} 2>&1; _ec=$?; [ $_ec -ne 0 ] && [ ! -s {output_file} ] && printf '[SSH exit %d] conexion o ejecucion fallo en {host}\\n' $_ec > {output_file}; touch {done_file}",
            extra_path = extra_path, host = shell_escape(&host), repo_path = shell_escape(&repo_path), output_file = output_file, done_file = done_file
        );
        Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    };

    // Write prompt to stdin, then close it so `$(cat)` and `cat |` receive EOF
    if let Some(ref mut child) = proc {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(actual_prompt.as_bytes());
        }
    }
    drop(proc.as_mut().and_then(|c| c.stdin.take()));

    let _ = app.emit("task-started", serde_json::json!({ "id": task.id, "target": task.target }));

    let task_id = task.id;
    let task_target = task.target.clone();
    let task_prompt = task.prompt.clone();
    let task_orchestrate = task.orchestrate;
    let task_started = task.started_at;
    let output_file_clone = output_file.clone();
    let done_file_clone = done_file.clone();
    let app_clone = app.clone();

    thread::spawn(move || {
        let mut child_proc = proc;
        let poll_start = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_secs(POLL_TIMEOUT_SECS);

        loop {
            thread::sleep(std::time::Duration::from_secs(3));

            // Check for timeout
            if poll_start.elapsed() >= timeout_duration {
                if let Some(ref mut child) = child_proc {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                let output = fs::read_to_string(&output_file_clone).unwrap_or_default();
                let _ = fs::remove_file(&output_file_clone);
                let _ = fs::remove_file(&done_file_clone);

                let store = app_clone.state::<TaskStore>();
                {
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.status = "timeout".to_string();
                        t.finished_at = Some(now_millis());
                        t.output = if output.is_empty() {
                            "[Timeout: tarea excedió 30 minutos]".to_string()
                        } else {
                            format!("{}\n\n[Timeout: tarea excedió 30 minutos]", output)
                        };
                    }
                }
                store.save_state();
                crate::app_logs::log_warn(&app_clone, format!("Action task #{} on '{}' timed out after 30 min", task_id, task_target));
                let _ = app_clone.emit("task-done", serde_json::json!({
                    "id": task_id,
                    "target": task_target,
                    "output": "[Timeout: tarea excedió 30 minutos]"
                }));
                crate::notifications::send_native(
                    &app_clone,
                    &format!("JARVIS - {}", task_target.to_uppercase()),
                    "Timeout: tarea excedió 30 minutos",
                );
                crate::whatsapp::notify_task_result(task_id, false, "[Timeout: tarea excedió 30 minutos]");
                break;
            }

            if std::path::Path::new(&done_file_clone).exists() {
                let output = fs::read_to_string(&output_file_clone).unwrap_or_default();
                let _ = fs::remove_file(&output_file_clone);
                let _ = fs::remove_file(&done_file_clone);

                log::info!("Action task #{} on '{}' completed ({} bytes output)", task_id, task_target, output.len());

                let store = app_clone.state::<TaskStore>();
                {
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.status = "done".to_string();
                        t.finished_at = Some(now_millis());
                        t.output = output.clone();
                    }
                }

                {
                    let mut history = store.conversation_history.lock().unwrap_or_else(|e| e.into_inner());
                    let entries = history.entry(task_target.clone()).or_insert_with(Vec::new);
                    entries.push(ConversationEntry {
                        id: task_id,
                        prompt: task_prompt.clone(),
                        output: output.clone(),
                    });
                    if entries.len() > MAX_HISTORY {
                        entries.remove(0);
                    }
                }

                store.save_state();

                // Extract [MEMORY] lines from output
                crate::messages::extract_memories_from_output(&output, &task_target);

                // Evaluate automation rules — use the same heuristic as dispatch_dependents
                let success = task_succeeded(&output);
                let trigger = if success { "on_task_complete" } else { "on_task_fail" };
                let rule_context = crate::rules::RuleContext {
                    target: task_target.clone(),
                    output: output.chars().take(2000).collect(),
                };
                let rule_actions = crate::rules::evaluate_rules(trigger, &rule_context, &app_clone);
                if !rule_actions.is_empty() {
                    crate::rules::execute_rule_actions(&app_clone, rule_actions);
                }

                // Webhook notification
                crate::webhooks::notify_task_complete(task_id, &task_target, success, &output);

                // Native notification
                let notif_title = format!("JARVIS - {}", task_target.to_uppercase());
                let notif_body = if success { "Tarea completada".to_string() } else { format!("Tarea fallida: {}", output.chars().take(80).collect::<String>()) };
                crate::notifications::send_native(&app_clone, &notif_title, &notif_body);
                // WhatsApp notification
                crate::whatsapp::notify_task_result(task_id, success, &output);

                // Save to task history
                let duration = task_started.map(|s| (now_millis() - s).max(0) as u64 / 1000).unwrap_or(0);
                crate::task_history::save_to_history(&app_clone, task_id, &task_target, &task_prompt, &output, success, duration);

                let output_preview2: String = output.chars().take(2000).collect();
                let _ = app_clone.emit("task-done", serde_json::json!({
                    "id": task_id,
                    "target": task_target,
                    "output": output_preview2
                }));

                if task_orchestrate {
                    if let Some(idx) = output.find("===PIXEL-TASK===") {
                        let pixel_prompt = output[idx + "===PIXEL-TASK===".len()..].trim();
                        if !pixel_prompt.is_empty() {
                            let pixel_task = send_task_internal(&app_clone, &store, "pixel", pixel_prompt, false);
                            let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                                t.pixel_task_id = Some(pixel_task.id);
                            }
                            store.save_state();
                        }
                    }
                }

                dispatch_dependents(&app_clone, &store, task_id, &output);

                break;
            } else if std::path::Path::new(&output_file_clone).exists() {
                if let Ok(partial) = fs::read_to_string(&output_file_clone) {
                    let store = app_clone.state::<TaskStore>();
                    let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.output = partial;
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn execute_action_impl(app: &AppHandle, store: &TaskStore, action: &str) -> Result<serde_json::Value, String> {
    match action {
        "git-pull" => {
            let registry = app.state::<crate::machines::MachineRegistry>();
            let (local_repo, remote_info) = {
                let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
                let local = machines.values().find(|m| m.host == "local" && m.enabled);
                let local_rp = local
                    .and_then(|m| m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()));
                let remote = machines.values().find(|m| m.host != "local" && m.enabled);
                let ri = remote.map(|m| {
                    let repo = m.repos.first().map(|r| r.path.clone()).or(m.repo_path.clone()).unwrap_or_default();
                    (m.host.clone(), repo)
                });
                (local_rp, ri)
            };

            let back = if let Some(ref rp) = local_repo {
                Command::new("git")
                    .args(["pull"])
                    .current_dir(rp)
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };

            let front = if let Some((ref host, ref repo)) = remote_info {
                Command::new("ssh")
                    .args([
                        "-o",
                        "ConnectTimeout=5",
                        host,
                        &format!("cd {} && git pull", shell_escape(repo)),
                    ])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };

            Ok(serde_json::json!({ "ok": true, "back": back, "front": front }))
        }
        "kill-all" => {
            // Only kill non-interactive JARVIS-spawned processes (claude -p / --print)
            // NEVER kill --continue or --resume (those are interactive Claude Code sessions)
            let _ = Command::new("pkill").args(["-INT", "-f", "claude -p "]).output();
            let _ = Command::new("pkill").args(["-INT", "-f", "claude --print"]).output();

            // Wait for graceful shutdown
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Force kill remaining JARVIS-spawned processes
            let _ = Command::new("pkill").args(["-9", "-f", "claude -p "]).output();
            let _ = Command::new("pkill").args(["-9", "-f", "claude --print"]).output();

            // Kill on all enabled remote machines (same: only -p/--print)
            let registry = app.state::<crate::machines::MachineRegistry>();
            let remote_hosts: Vec<String> = {
                let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
                machines.values()
                    .filter(|m| m.host != "local" && m.enabled)
                    .map(|m| m.host.clone())
                    .collect()
            };
            let kill_script = "pkill -INT -f 'claude -p ' 2>/dev/null; pkill -INT -f 'claude --print' 2>/dev/null; sleep 1; pkill -9 -f 'claude -p ' 2>/dev/null; pkill -9 -f 'claude --print' 2>/dev/null";
            for host in &remote_hosts {
                let _ = Command::new("ssh")
                    .args([
                        "-o", "ConnectTimeout=3",
                        "-o", "ServerAliveInterval=30",
                        host,
                        kill_script,
                    ])
                    .output();
            }

            // 4. Mark all running/pending tasks as done
            {
                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                for t in tasks.iter_mut() {
                    if t.status == "running" || t.status == "pending" {
                        let output_file = format!("/tmp/jarvis-task-{}.out", t.id);
                        let done_file = format!("/tmp/jarvis-task-{}.done", t.id);
                        if std::path::Path::new(&done_file).exists() {
                            t.output = fs::read_to_string(&output_file).unwrap_or_default();
                        } else if t.output.is_empty() {
                            t.output = "[Killed by MATAR TODO]".to_string();
                        }
                        let _ = fs::remove_file(&output_file);
                        let _ = fs::remove_file(&done_file);
                        t.status = "killed".to_string();
                        t.finished_at = Some(now_millis());
                    }
                }
            }

            // 5. Clean up all /tmp/jarvis-task-* files
            if let Ok(entries) = fs::read_dir("/tmp") {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("jarvis-task-") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }

            store.save_state();
            Ok(serde_json::json!({ "ok": true }))
        }
        "clear-history" => {
            // Full clean: clear conversation history, all tasks, and tmp files
            {
                let mut history = store.conversation_history.lock().unwrap_or_else(|e| e.into_inner());
                history.clear();
            }
            {
                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                // Kill any still-running before clearing
                for t in tasks.iter() {
                    if t.status == "running" {
                        let _ = fs::remove_file(format!("/tmp/jarvis-task-{}.out", t.id));
                        let _ = fs::remove_file(format!("/tmp/jarvis-task-{}.done", t.id));
                    }
                }
                tasks.clear();
            }
            // Clean all /tmp/jarvis-task-* files
            if let Ok(entries) = fs::read_dir("/tmp") {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("jarvis-task-") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
            store.save_state();
            Ok(serde_json::json!({ "ok": true }))
        }
        a if a.starts_with("kill-") => {
            let machine_id = &a[5..]; // e.g. "atlas", "pixel"
            let registry = app.state::<crate::machines::MachineRegistry>();
            let machine_host = {
                let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
                machines.get(machine_id).map(|m| m.host.clone())
            };
            // Only kill JARVIS-spawned processes (claude -p / --print)
            // NEVER kill --continue or --resume (interactive Claude Code sessions)
            let kill_script = "pkill -INT -f 'claude -p ' 2>/dev/null; pkill -INT -f 'claude --print' 2>/dev/null; sleep 1; pkill -9 -f 'claude -p ' 2>/dev/null; pkill -9 -f 'claude --print' 2>/dev/null";
            match machine_host {
                Some(ref host) if host == "local" => {
                    let _ = Command::new("pkill").args(["-INT", "-f", "claude -p "]).output();
                    let _ = Command::new("pkill").args(["-INT", "-f", "claude --print"]).output();
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    let _ = Command::new("pkill").args(["-9", "-f", "claude -p "]).output();
                    let _ = Command::new("pkill").args(["-9", "-f", "claude --print"]).output();
                }
                Some(ref host) => {
                    let _ = Command::new("ssh")
                        .args([
                            "-o", "ConnectTimeout=3",
                            "-o", "ServerAliveInterval=30",
                            host,
                            kill_script,
                        ])
                        .output();
                }
                None => return Err(format!("maquina no encontrada: {}", machine_id)),
            }
            // Mark tasks targeting this machine as killed
            {
                let mut tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                for t in tasks.iter_mut() {
                    if (t.status == "running" || t.status == "pending") && t.target == machine_id {
                        t.status = "killed".to_string();
                        t.finished_at = Some(now_millis());
                        if t.output.is_empty() {
                            t.output = format!("[Killed on {}]", machine_id);
                        }
                    }
                }
            }
            store.save_state();
            Ok(serde_json::json!({ "ok": true, "machine": machine_id }))
        }
        _ => Err(format!("accion desconocida: {}", action)),
    }
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn send_task(
    app: AppHandle,
    store: tauri::State<'_, TaskStore>,
    target: String,
    prompt: String,
    orchestrate: Option<bool>,
    repo: Option<String>,
) -> Result<Task, String> {
    if prompt.trim().is_empty() {
        return Err("prompt vacio".into());
    }
    if prompt.len() > 1_000_000 {
        return Err("Prompt too large (max 1MB)".into());
    }
    // Allow routing keywords "both" and "auto", plus any machine ID registered in the registry.
    // This avoids hardcoding machine IDs so users with different machine names are not rejected.
    let is_routing_token = ["both", "auto"].contains(&target.as_str());
    let is_known_machine = {
        let registry = app.state::<crate::machines::MachineRegistry>();
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.contains_key(target.as_str())
    };
    if target.is_empty() || (!is_routing_token && !is_known_machine) {
        return Err(format!("target invalido: '{}'", target));
    }

    let (actual_target, _reason) = if target == "auto" {
        let result = resolve_smart_target(&store, &prompt);
        let _ = app.emit("auto-routed", serde_json::json!({
            "target": result.0,
            "reason": result.1,
        }));
        result
    } else if target == "both" {
        // "both" mode: send to the local/orchestrator machine (with orchestration enabled),
        // which will then forward a task to the remote machine via ===PIXEL-TASK=== extraction.
        // Use the first enabled local machine; fall back to "atlas" for backward compatibility.
        let local_id = {
            let registry = app.state::<crate::machines::MachineRegistry>();
            let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
            machines.values()
                .find(|m| m.host == "local" && m.enabled)
                .map(|m| m.id.clone())
                .unwrap_or_else(|| "atlas".to_string())
        };
        (local_id, String::new())
    } else {
        (target.clone(), String::new())
    };

    let orch = orchestrate.unwrap_or(false) || target == "both";
    let repo_ref = repo.as_deref();

    let task = send_task_internal_with_repo(&app, &store, &actual_target, prompt.trim(), orch, repo_ref, Vec::new(), "on_success".to_string());
    Ok(task)
}

#[tauri::command]
pub fn get_tasks(store: tauri::State<'_, TaskStore>) -> Vec<Task> {
    let tasks = store.tasks.lock().unwrap_or_else(|e| e.into_inner());
    let len = tasks.len();
    let start = len.saturating_sub(20);
    tasks[start..].to_vec()
}

#[tauri::command]
pub fn execute_action(
    app: AppHandle,
    store: tauri::State<'_, TaskStore>,
    action: String,
) -> Result<serde_json::Value, String> {
    if action.len() > 10_000 {
        return Err("Action too large (max 10KB)".into());
    }
    execute_action_impl(&app, &store, &action)
}

#[tauri::command]
pub fn get_config(store: tauri::State<'_, TaskStore>) -> Config {
    store.config.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn set_config(
    store: tauri::State<'_, TaskStore>,
    config: Config,
) -> Config {
    let mut c = store.config.lock().unwrap_or_else(|e| e.into_inner());
    // Only overwrite a stored field when the incoming value is non-empty.
    // Using `||` would allow an empty incoming value to clear a valid stored field
    // whenever the stored field happens to also be empty (no-op but misleading).
    if !config.session_id.is_empty() {
        c.session_id = config.session_id.trim().to_string();
    }
    if !config.rama.is_empty() {
        c.rama = config.rama.trim().to_string();
    }
    if !config.objetivo.is_empty() {
        c.objetivo = config.objetivo.trim().to_string();
    }
    let result = c.clone();
    drop(c);
    store.save_state();
    result
}

#[tauri::command]
pub fn send_task_chain(
    app: AppHandle,
    store: tauri::State<'_, TaskStore>,
    steps: Vec<TaskChainStep>,
) -> Result<Vec<Task>, String> {
    if steps.is_empty() {
        return Err("cadena vacia".into());
    }
    // Validate all steps up front before creating any tasks
    {
        let registry = app.state::<crate::machines::MachineRegistry>();
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        for (i, step) in steps.iter().enumerate() {
            if step.prompt.trim().is_empty() {
                return Err(format!("Step {} prompt vacio", i));
            }
            if step.prompt.len() > 100_000 {
                return Err(format!("Step {} prompt too large (max 1MB)", i));
            }
            if step.target.is_empty() || !machines.contains_key(step.target.as_str()) {
                return Err(format!("Step {} target invalido: '{}'", i, step.target));
            }
        }
    }

    let mut created_tasks: Vec<Task> = Vec::new();

    for (i, step) in steps.iter().enumerate() {
        let depends_on = if i == 0 {
            Vec::new()
        } else {
            vec![created_tasks[i - 1].id]
        };

        let task = send_task_internal_with_deps(
            &app,
            &store,
            &step.target,
            step.prompt.trim(),
            false,
            depends_on,
            step.run_condition.clone(),
        );
        created_tasks.push(task);
    }

    Ok(created_tasks)
}

#[tauri::command]
pub fn send_task_graph(
    app: AppHandle,
    store: tauri::State<'_, TaskStore>,
    graph: TaskGraph,
) -> Result<Vec<i64>, String> {
    if graph.nodes.is_empty() {
        return Err("graph vacio".into());
    }
    if graph.nodes.len() > 50 {
        return Err("graph demasiado grande (max 50 nodos)".into());
    }

    // Validate node IDs are unique and non-empty
    let mut seen_ids = std::collections::HashSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty() {
            return Err("node id vacio".into());
        }
        if !seen_ids.insert(node.id.clone()) {
            return Err(format!("node id duplicado: '{}'", node.id));
        }
        if node.prompt.trim().is_empty() {
            return Err(format!("node '{}' prompt vacio", node.id));
        }
        if node.prompt.len() > 1_000_000 {
            return Err(format!("node '{}' prompt too large (max 1MB)", node.id));
        }
    }

    // Validate all targets exist
    {
        let registry = app.state::<crate::machines::MachineRegistry>();
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        for node in &graph.nodes {
            if node.target.is_empty() || !machines.contains_key(node.target.as_str()) {
                return Err(format!("node '{}' target invalido: '{}'", node.id, node.target));
            }
        }
    }

    // Validate all depends_on references point to real node IDs
    for node in &graph.nodes {
        for dep in &node.depends_on {
            if !seen_ids.contains(dep) {
                return Err(format!("node '{}' depends on unknown node '{}'", node.id, dep));
            }
            if dep == &node.id {
                return Err(format!("node '{}' depends on itself", node.id));
            }
        }
    }

    // Cycle detection via topological sort (Kahn's algorithm)
    {
        let mut in_degree: HashMap<String, usize> = graph.nodes.iter().map(|n| (n.id.clone(), 0)).collect();
        let mut adj: HashMap<String, Vec<String>> = graph.nodes.iter().map(|n| (n.id.clone(), Vec::new())).collect();
        for node in &graph.nodes {
            for dep in &node.depends_on {
                *in_degree.entry(node.id.clone()).or_insert(0) += 1;
                adj.entry(dep.clone()).or_default().push(node.id.clone());
            }
        }
        let mut queue: std::collections::VecDeque<String> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut processed = 0usize;
        while let Some(node_id) = queue.pop_front() {
            processed += 1;
            if let Some(neighbors) = adj.get(&node_id) {
                for neighbor in neighbors {
                    let deg = in_degree.entry(neighbor.clone()).or_insert(0);
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        if processed != graph.nodes.len() {
            return Err("graph contiene ciclos".into());
        }
    }

    // Map node string IDs -> task u64 IDs as we create them
    let mut node_id_to_task_id: HashMap<String, u64> = HashMap::new();
    let mut created_task_ids: Vec<i64> = Vec::new();

    // Process in topological order: nodes with no deps first, then their dependents
    // We iterate until all nodes are created. Because we validated no cycles, this terminates.
    let mut remaining: Vec<&crate::types::TaskGraphNode> = graph.nodes.iter().collect();
    let mut iteration_limit = graph.nodes.len() * graph.nodes.len() + 1;
    while !remaining.is_empty() {
        let before_len = remaining.len();
        remaining.retain(|node| {
            // Check if all deps have been created already
            let all_deps_created = node.depends_on.iter().all(|dep_id| node_id_to_task_id.contains_key(dep_id));
            if !all_deps_created {
                return true; // keep in remaining
            }

            let depends_on_ids: Vec<u64> = node.depends_on.iter()
                .map(|dep_id| node_id_to_task_id[dep_id])
                .collect();

            // Map on_failure to run_condition for the task
            // "stop"             -> "on_success" (don't run if a dep failed)
            // "continue"         -> "always"     (run regardless)
            // "skip_dependents"  -> "on_success"  (same as stop; we mark as done-skipped in dispatch_dependents)
            let run_condition = match node.on_failure.as_str() {
                "continue" => "always".to_string(),
                _ => "on_success".to_string(),
            };

            let task = send_task_internal_with_deps(
                &app,
                &store,
                &node.target,
                node.prompt.trim(),
                false,
                depends_on_ids,
                run_condition,
            );

            node_id_to_task_id.insert(node.id.clone(), task.id);
            created_task_ids.push(task.id as i64);
            false // remove from remaining
        });

        // Safety: if no progress was made, break (shouldn't happen after cycle check)
        if remaining.len() == before_len {
            iteration_limit -= 1;
            if iteration_limit == 0 {
                break;
            }
        }
    }

    Ok(created_task_ids)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // now_millis
    // -----------------------------------------------------------------------

    #[test]
    fn now_millis_is_positive() {
        assert!(now_millis() > 0);
    }

    #[test]
    fn now_millis_is_plausible_epoch() {
        // 2020-01-01 00:00:00 UTC in milliseconds
        let lower_bound: i64 = 1_577_836_800_000;
        // 2100-01-01 — far future sanity ceiling
        let upper_bound: i64 = 4_102_444_800_000;
        let ts = now_millis();
        assert!(
            ts > lower_bound && ts < upper_bound,
            "now_millis() = {} is outside plausible range",
            ts
        );
    }

    #[test]
    fn now_millis_increases_monotonically() {
        let t1 = now_millis();
        // Spin briefly to give the clock time to tick
        let mut sum = 0i64;
        for i in 0..100_000 {
            sum += i;
        }
        let _ = sum; // prevent optimisation
        let t2 = now_millis();
        assert!(t2 >= t1, "now_millis did not increase: t1={} t2={}", t1, t2);
    }

    // -----------------------------------------------------------------------
    // task_succeeded
    // -----------------------------------------------------------------------

    #[test]
    fn task_succeeded_clean_output_returns_true() {
        assert!(task_succeeded("All done. 5 tests passed."));
    }

    #[test]
    fn task_succeeded_empty_output_returns_false() {
        assert!(!task_succeeded(""));
    }

    #[test]
    fn task_succeeded_whitespace_only_returns_false() {
        assert!(!task_succeeded("   \n\t  "));
    }

    #[test]
    fn task_succeeded_contains_error_colon_returns_false() {
        assert!(!task_succeeded("error: undefined variable `foo`"));
    }

    #[test]
    fn task_succeeded_error_colon_uppercase_returns_false() {
        // heuristic is case-insensitive (to_lowercase)
        assert!(!task_succeeded("Error: file not found"));
    }

    #[test]
    fn task_succeeded_contains_fatal_colon_returns_false() {
        assert!(!task_succeeded("fatal: not a git repository"));
    }

    #[test]
    fn task_succeeded_fatal_uppercase_returns_false() {
        assert!(!task_succeeded("FATAL: out of memory"));
    }

    #[test]
    fn task_succeeded_contains_panicked_at_returns_false() {
        assert!(!task_succeeded(
            "thread 'main' panicked at 'index out of bounds', src/main.rs:10"
        ));
    }

    #[test]
    fn task_succeeded_error_word_without_colon_is_ok() {
        // "error" alone (no colon) should NOT trigger the heuristic
        assert!(task_succeeded("no errors were found in the output"));
    }

    #[test]
    fn task_succeeded_fatal_word_without_colon_is_ok() {
        // "fatal" alone should NOT trigger the heuristic
        assert!(task_succeeded("fatal blow to the test suite (just kidding, all pass)"));
    }

    #[test]
    fn task_succeeded_multiline_with_error_colon_returns_false() {
        let output = "Step 1: OK\nStep 2: OK\nerror: build failed\nStep 3: skipped";
        assert!(!task_succeeded(output));
    }

    #[test]
    fn task_succeeded_multiline_clean_returns_true() {
        let output = "Step 1: OK\nStep 2: OK\nStep 3: OK\nAll steps completed.";
        assert!(task_succeeded(output));
    }

    // -----------------------------------------------------------------------
    // PIXEL-TASK extraction pattern (mirrors the inline logic in the polling
    // thread: output.find("===PIXEL-TASK==="))
    // -----------------------------------------------------------------------

    #[test]
    fn pixel_task_extraction_finds_marker() {
        let output = "Backend done.\n===PIXEL-TASK===\nBuild a login form.";
        let marker = "===PIXEL-TASK===";
        let result = output.find(marker).map(|idx| output[idx + marker.len()..].trim().to_string());
        assert_eq!(result, Some("Build a login form.".to_string()));
    }

    #[test]
    fn pixel_task_extraction_missing_marker_returns_none() {
        let output = "Backend done. No pixel task here.";
        let marker = "===PIXEL-TASK===";
        let result = output.find(marker).map(|idx| output[idx + marker.len()..].trim().to_string());
        assert!(result.is_none());
    }

    #[test]
    fn pixel_task_extraction_empty_payload_after_marker() {
        let output = "===PIXEL-TASK===   \n  ";
        let marker = "===PIXEL-TASK===";
        let payload = output.find(marker).map(|idx| output[idx + marker.len()..].trim().to_string()).unwrap_or_default();
        // Payload should trim to empty — callers guard with `!pixel_prompt.is_empty()`
        assert!(payload.is_empty());
    }

    #[test]
    fn pixel_task_extraction_only_first_match_used() {
        // The code uses `output.find()` which returns the first occurrence.
        let output = "===PIXEL-TASK===\nFirst task.\n===PIXEL-TASK===\nSecond task.";
        let marker = "===PIXEL-TASK===";
        let payload = output.find(marker).map(|idx| output[idx + marker.len()..].trim().to_string()).unwrap_or_default();
        // Should include everything after the first marker (both tasks in the slice)
        assert!(payload.starts_with("First task."));
    }

    // -----------------------------------------------------------------------
    // numstat status determination (mirrors the inline logic in get_git_diff)
    // -----------------------------------------------------------------------

    fn numstat_status(adds: u32, dels: u32) -> &'static str {
        if adds > 0 && dels > 0 { "modified" }
        else if adds > 0 { "added" }
        else { "deleted" }
    }

    #[test]
    fn numstat_status_adds_and_dels_gives_modified() {
        assert_eq!(numstat_status(10, 3), "modified");
    }

    #[test]
    fn numstat_status_only_adds_gives_added() {
        assert_eq!(numstat_status(5, 0), "added");
    }

    #[test]
    fn numstat_status_only_dels_gives_deleted() {
        assert_eq!(numstat_status(0, 7), "deleted");
    }

    #[test]
    fn numstat_status_both_zero_gives_deleted() {
        // Edge case: 0 adds, 0 dels — falls through to "deleted"
        assert_eq!(numstat_status(0, 0), "deleted");
    }

    #[test]
    fn numstat_status_one_add_one_del_gives_modified() {
        assert_eq!(numstat_status(1, 1), "modified");
    }

    // -----------------------------------------------------------------------
    // run_condition / should_run logic (mirrors dispatch_dependents)
    // -----------------------------------------------------------------------

    fn evaluate_run_condition(condition: &str, success: bool) -> bool {
        match condition {
            "always" => true,
            "on_failure" => !success,
            _ => success, // "on_success" is the default
        }
    }

    #[test]
    fn run_condition_always_runs_on_success() {
        assert!(evaluate_run_condition("always", true));
    }

    #[test]
    fn run_condition_always_runs_on_failure() {
        assert!(evaluate_run_condition("always", false));
    }

    #[test]
    fn run_condition_on_success_runs_when_succeeded() {
        assert!(evaluate_run_condition("on_success", true));
    }

    #[test]
    fn run_condition_on_success_skips_when_failed() {
        assert!(!evaluate_run_condition("on_success", false));
    }

    #[test]
    fn run_condition_on_failure_runs_when_failed() {
        assert!(evaluate_run_condition("on_failure", false));
    }

    #[test]
    fn run_condition_on_failure_skips_when_succeeded() {
        assert!(!evaluate_run_condition("on_failure", true));
    }

    #[test]
    fn run_condition_unknown_falls_back_to_on_success_semantics() {
        // Any unrecognised value is treated the same as "on_success"
        assert!(evaluate_run_condition("custom_condition", true));
        assert!(!evaluate_run_condition("custom_condition", false));
    }

    // -----------------------------------------------------------------------
    // Output preview truncation (mirrors the 2000-char preview used in
    // task-done events and the 80-char error snippet in notifications)
    // -----------------------------------------------------------------------

    #[test]
    fn output_preview_truncates_at_2000_chars() {
        let long_output = "x".repeat(5000);
        let preview: String = long_output.chars().take(2000).collect();
        assert_eq!(preview.len(), 2000);
    }

    #[test]
    fn output_preview_shorter_than_limit_is_returned_fully() {
        let short_output = "hello world";
        let preview: String = short_output.chars().take(2000).collect();
        assert_eq!(preview, short_output);
    }

    #[test]
    fn error_snippet_truncates_at_80_chars() {
        let long_error = "error: ".to_string() + &"x".repeat(200);
        let snippet: String = long_error.chars().take(80).collect();
        assert_eq!(snippet.chars().count(), 80);
    }

    // -----------------------------------------------------------------------
    // MAX_HISTORY constant
    // -----------------------------------------------------------------------

    #[test]
    fn max_history_is_ten() {
        assert_eq!(MAX_HISTORY, 10);
    }

    // -----------------------------------------------------------------------
    // POLL_TIMEOUT_SECS constant
    // -----------------------------------------------------------------------

    #[test]
    fn poll_timeout_is_thirty_minutes() {
        assert_eq!(POLL_TIMEOUT_SECS, 30 * 60);
    }
}
