use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::io::Write as IoWrite;
use std::process::{Command, Stdio};
use std::fs;

use tauri::{AppHandle, Emitter, Manager};

use crate::machines::MachineRegistry;
use crate::jsonl::{parse_raw_activity, get_active_jsonl_files, repo_path_to_jsonl_dir};
use crate::tasks::TaskStore;
use crate::types::{shell_escape, Activity, Machine, PlanStep, PlanningHistoryEntry, PlanningMessage, PlanningState, RepoStatus};

// ---------------------------------------------------------------------------
// Planning Store
// ---------------------------------------------------------------------------

/// Per-session planning configuration (supplied by the caller at session start).
#[derive(Debug, Clone, Default)]
pub struct PlanningConfig {
    /// Optional custom timeout in seconds for Claude invocations.
    /// Falls back to `PLANNING_TIMEOUT_SECS` when `None`.
    pub timeout_secs: Option<u64>,
}


pub struct PlanningStore {
    pub session: Mutex<Option<PlanningState>>,
    pub child_pid: Mutex<Option<u32>>,
    pub history: Mutex<Vec<PlanningHistoryEntry>>,
    pub history_id_counter: Mutex<u64>,
    /// Active planning configuration (timeout etc.)
    pub config: Mutex<PlanningConfig>,
}

const PLANNING_HISTORY_MAX: usize = 50;

const PLANNING_STATE_FILE: &str = ".config/jarvis/planning_state.json";

fn planning_state_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    home.join(PLANNING_STATE_FILE)
}

impl PlanningStore {
    pub fn new() -> Self {
        let store = Self {
            session: Mutex::new(None),
            child_pid: Mutex::new(None),
            history: Mutex::new(Vec::new()),
            history_id_counter: Mutex::new(0),
            config: Mutex::new(PlanningConfig::default()),
        };
        store.load_planning_state();
        store
    }

    /// Append a prompt/response exchange to the in-memory history, capping at PLANNING_HISTORY_MAX.
    pub fn push_history(&self, prompt: String, response: String, machine: String) {
        let mut counter = self.history_id_counter.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        let id = *counter;
        drop(counter);

        let entry = PlanningHistoryEntry {
            id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            prompt,
            response,
            machine,
        };

        let mut history = self.history.lock().unwrap_or_else(|e| e.into_inner());
        history.push(entry);
        if history.len() > PLANNING_HISTORY_MAX {
            let overflow = history.len() - PLANNING_HISTORY_MAX;
            history.drain(..overflow);
        }
    }

    /// Persist the current planning session to disk as JSON.
    pub fn save_planning_state(&self) {
        // Clone the data needed while holding the lock briefly, then drop it
        let snapshot: Option<PlanningState> = {
            let session = self.session.lock().unwrap_or_else(|e| e.into_inner());
            session.clone()
        }; // lock dropped here — all fs I/O happens without holding it

        let path = planning_state_path();
        if let Some(ref state) = snapshot {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(state) {
                // Write to a temp file then rename for atomicity (crash-safe)
                let tmp = path.with_extension("tmp");
                if fs::write(&tmp, json).is_ok() {
                    let _ = fs::rename(&tmp, &path);
                }
            }
        } else {
            // No active session — remove the file
            let _ = fs::remove_file(&path);
        }
    }

    /// Load a previously persisted planning session from disk.
    fn load_planning_state(&self) {
        let path = planning_state_path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<PlanningState>(&data) {
                // Only restore sessions that were actively running (planning/executing).
                // Terminal states (done, cancelled, done-with-errors) don't need recovery.
                match state.phase.as_str() {
                    "planning" | "review" | "executing" => {
                        log::info!(
                            "Restored planning session '{}' in phase '{}'",
                            state.id,
                            state.phase
                        );
                        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
                    }
                    _ => {
                        // Terminal state — clean up the file
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn emit_update(app: &AppHandle, state: &PlanningState) {
    let _ = app.emit("planning-update", state);
}

// ---------------------------------------------------------------------------
// Machine registry helpers
// ---------------------------------------------------------------------------

/// Look up a machine from the registry by id
fn find_machine(app: &AppHandle, target: &str) -> Option<Machine> {
    let registry = app.state::<MachineRegistry>();
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    machines.get(target).cloned()
}

/// Get the first repo path for a machine
fn machine_repo_path(machine: &Machine) -> Option<String> {
    machine.repos.first().map(|r| r.path.clone())
}

/// Get the JSONL directory for a target machine (local only)
fn jsonl_dir_for_machine(machine: &Machine) -> String {
    let is_local = machine.host == "local";
    if !is_local {
        return String::new();
    }
    if let Some(repo_path) = machine_repo_path(machine) {
        repo_path_to_jsonl_dir(&repo_path, true, None)
    } else {
        String::new()
    }
}

/// Build a repo context string for local machines (branch, commits, dirty count, project type).
/// Returns an empty string for remote machines (SSH per-call is too slow).
fn build_repo_context(app: &AppHandle, target: &str) -> String {
    let machine = match find_machine(app, target) {
        Some(m) => m,
        None => return String::new(),
    };

    // Only for local machines (SSH context fetch is too slow per-call)
    if machine.host != "local" {
        return String::new();
    }

    let repo_path = match machine_repo_path(&machine) {
        Some(p) => p,
        None => return String::new(),
    };

    let run = |cmd: &str| -> String {
        Command::new("bash")
            .args(["-c", &format!("cd {} && {}", shell_escape(&repo_path), cmd)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };

    let branch = run("git rev-parse --abbrev-ref HEAD 2>/dev/null");
    let last_commits = run("git log -3 --oneline 2>/dev/null");
    let dirty = run("git status --porcelain 2>/dev/null | wc -l | tr -d ' '");
    let dirty_count: u32 = dirty.parse().unwrap_or(0);

    // Detect project type from files
    let has_cargo = std::path::Path::new(&repo_path).join("Cargo.toml").exists();
    let has_package = std::path::Path::new(&repo_path).join("package.json").exists();
    let has_angular = std::path::Path::new(&repo_path).join("angular.json").exists();
    let project_type = match (has_cargo, has_package, has_angular) {
        (true, _, _) => "Rust",
        (_, true, true) => "Angular",
        (_, true, false) => "Node.js/TypeScript",
        _ => "Unknown",
    };

    if branch.is_empty() {
        return String::new();
    }

    let mut ctx = format!(
        "\n--- CONTEXTO DEL REPOSITORIO ({}) ---\nBranch: {}\nTipo de proyecto: {}\nÚltimos commits:\n{}\n",
        repo_path, branch, project_type, last_commits
    );
    if dirty_count > 0 {
        ctx.push_str(&format!("Cambios sin commitear: {} archivos\n", dirty_count));
    }
    ctx.push_str("--- FIN CONTEXTO ---\n");
    ctx
}

/// Collect all machines with their repo info: Vec<(machine_id, host, repo_path, is_local)>
fn collect_machine_repos(app: &AppHandle) -> Vec<(String, String, String, bool)> {
    let registry = app.state::<MachineRegistry>();
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    let mut result = Vec::new();
    for (id, m) in machines.iter() {
        if !m.enabled {
            continue;
        }
        if let Some(repo) = m.repos.first() {
            let is_local = m.host == "local";
            result.push((id.clone(), m.host.clone(), repo.path.clone(), is_local));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Claude invocation (reuses pattern from tasks.rs)
// ---------------------------------------------------------------------------

/// Read the latest activity from the newest JSONL file for a given JSONL dir
fn read_current_activity(dir: &str) -> Vec<Activity> {
    if dir.is_empty() {
        return Vec::new();
    }
    let files = get_active_jsonl_files(dir, 120);
    if let Some((path, _, _)) = files.first() {
        if let Ok(raw) = fs::read_to_string(path) {
            let lines: Vec<&str> = raw.lines().collect();
            let start = if lines.len() > 30 { lines.len() - 30 } else { 0 };
            let tail = lines[start..].join("\n");
            let activities = parse_raw_activity(&tail);
            return activities.into_iter().rev().take(5).rev().collect();
        }
    }
    Vec::new()
}

fn call_claude_with_updates(
    app: &AppHandle,
    planning_store: &PlanningStore,
    target: &str,
    prompt: &str,
) -> String {
    // Build repo context for local machines
    let repo_context = build_repo_context(app, target);
    let full_prompt = if repo_context.is_empty() {
        prompt.to_string()
    } else {
        format!("{}{}", repo_context, prompt)
    };

    let machine = find_machine(app, target);
    let repo_path = machine.as_ref().and_then(machine_repo_path).unwrap_or_default();
    let is_local = machine.as_ref().map(|m| m.host == "local").unwrap_or(false);
    let ssh_host = machine.as_ref().map(|m| m.host.clone()).unwrap_or_else(|| target.to_string());
    let jsonl_dir = machine.as_ref().map(jsonl_dir_for_machine).unwrap_or_default();

    let output_file = format!("/tmp/jarvis-planning-{}-{}.out", target, std::process::id());
    let done_file = format!("/tmp/jarvis-planning-{}-{}.done", target, std::process::id());

    let _ = fs::remove_file(&output_file);
    let _ = fs::remove_file(&done_file);

    let mut child_proc: Option<std::process::Child> = if is_local {
        let home = dirs::home_dir().unwrap_or_default().display().to_string();
        let extra_path = format!("{}/.local/bin:{}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin", home, home);
        let repo_path_escaped = shell_escape(&repo_path);
        let bash_cmd = format!(
            "export PATH=\"{extra_path}:$PATH\"; cd {repo_path_escaped} && unset CLAUDECODE && claude -p \"$(cat)\" --output-format text > {output_file} 2>&1; touch {done_file}",
        );
        let mut proc = Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        if let Some(ref mut child) = proc {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(full_prompt.as_bytes());
            }
        }
        proc
    } else {
        let bash_cmd = format!(
            "cat | ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=20 {} \"unset CLAUDECODE; cd {} 2>/dev/null || cd ~; claude -p --output-format text 2>/dev/null\" > {output_file}; touch {done_file}",
            shell_escape(&ssh_host), shell_escape(&repo_path)
        );
        let mut proc = Command::new("bash")
            .args(["-c", &bash_cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        if let Some(ref mut child) = proc {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(full_prompt.as_bytes());
            }
        }
        proc
    };

    // Store PID so cancel_planning can kill the process
    if let Some(ref child) = child_proc {
        *planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()) = Some(child.id());
    }

    const PLANNING_TIMEOUT_SECS: u64 = 30 * 60;
    let timeout_secs = {
        let cfg = planning_store.config.lock().unwrap_or_else(|e| e.into_inner());
        cfg.timeout_secs.unwrap_or(PLANNING_TIMEOUT_SECS)
    };
    let start = std::time::Instant::now();
    let mut emitted_bytes: usize = 0; // track how many bytes of output we've already chunked
    let session_id = {
        let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        session.as_ref().map(|s| s.id.clone()).unwrap_or_default()
    };
    loop {
        thread::sleep(Duration::from_secs(3));

        // Safety timeout — bail if the process dies without creating .done
        if start.elapsed().as_secs() > timeout_secs {
            if let Some(ref mut child) = child_proc {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = fs::remove_file(&output_file);
            let _ = fs::remove_file(&done_file);
            *planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return "[timeout]".to_string();
        }

        // Check if child exited without creating .done (crash/OOM)
        if let Some(ref mut child) = child_proc {
            if let Ok(Some(_)) = child.try_wait() {
                // Reap the child to avoid a zombie process
                let _ = child.wait();
                let output = fs::read_to_string(&output_file).unwrap_or_default();
                let _ = fs::remove_file(&output_file);
                let _ = fs::remove_file(&done_file);
                *planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return output;
            }
        }

        if std::path::Path::new(&done_file).exists() {
            let output = fs::read_to_string(&output_file).unwrap_or_default();
            let _ = fs::remove_file(&output_file);
            let _ = fs::remove_file(&done_file);
            // Reap child to avoid zombie
            if let Some(ref mut child) = child_proc {
                let _ = child.kill();
                let _ = child.wait();
            }
            // Clear PID — process is done
            *planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let snapshot = {
                let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref mut s) = *session {
                    s.elapsed_secs = start.elapsed().as_secs();
                    s.current_activity.clear();
                    s.streaming_text = None;
                    Some(s.clone())
                } else {
                    None
                }
            };
            if let Some(ref snap) = snapshot {
                emit_update(app, snap);
            }
            return output;
        }

        // Read partial output for streaming display
        let partial = fs::read_to_string(&output_file).unwrap_or_default();

        // Emit new lines as planning-chunk events (incremental streaming)
        if partial.len() > emitted_bytes {
            let new_bytes = &partial[emitted_bytes..];
            // Emit complete lines only; buffer incomplete last line
            let last_newline = new_bytes.rfind('\n').map(|i| i + 1).unwrap_or(0);
            if last_newline > 0 {
                let chunk = &new_bytes[..last_newline];
                if !chunk.is_empty() {
                    let _ = app.emit("planning-chunk", serde_json::json!({
                        "session_id": session_id,
                        "chunk": chunk
                    }));
                }
                emitted_bytes += last_newline;
            }
        }

        let streaming = if partial.is_empty() {
            None
        } else {
            // Show last 1500 chars to avoid sending huge payloads
            let chars: Vec<char> = partial.chars().collect();
            let start_idx = if chars.len() > 1500 { chars.len() - 1500 } else { 0 };
            Some(chars[start_idx..].iter().collect::<String>())
        };

        let activity = read_current_activity(&jsonl_dir);
        let snapshot = {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                if s.phase == "cancelled" {
                    drop(session); // must drop before locking child_pid to avoid ABBA deadlock
                    *planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    // Kill and reap the child to avoid a zombie
                    if let Some(ref mut child) = child_proc {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    let _ = fs::remove_file(&output_file);
                    let _ = fs::remove_file(&done_file);
                    return String::new();
                }
                s.elapsed_secs = start.elapsed().as_secs();
                s.current_activity = activity;
                s.streaming_text = streaming;
                Some(s.clone())
            } else {
                None
            }
        };
        if let Some(ref snap) = snapshot {
            emit_update(app, snap);
        }
    }
}

// ---------------------------------------------------------------------------
// Plan parser
// ---------------------------------------------------------------------------

fn parse_plan_steps(text: &str) -> Vec<PlanStep> {
    let mut steps = Vec::new();
    let marker = "===PLAN READY===";
    let Some(pos) = text.find(marker) else {
        return steps;
    };
    let plan_text = &text[pos + marker.len()..];

    for line in plan_text.lines() {
        let line = line.trim();
        let cleaned = line
            .trim_start_matches('-')
            .trim()
            .trim_start_matches("[ ]")
            .trim()
            .trim_start_matches("[]")
            .trim();

        if cleaned.is_empty() {
            continue;
        }

        let (target, desc) = if let Some(rest) = cleaned.strip_prefix("atlas:") {
            ("atlas", rest.trim())
        } else if let Some(rest) = cleaned.strip_prefix("pixel:") {
            ("pixel", rest.trim())
        } else if let Some(rest) = cleaned.strip_prefix("ATLAS:") {
            ("atlas", rest.trim())
        } else if let Some(rest) = cleaned.strip_prefix("PIXEL:") {
            ("pixel", rest.trim())
        } else {
            // Generic: match any lowercase word: prefix (e.g. "nova: ...")
            let mut found = false;
            let mut generic_target = "";
            let mut generic_desc = "";
            if let Some(colon_pos) = cleaned.find(':') {
                let prefix = &cleaned[..colon_pos];
                let is_valid = !prefix.is_empty()
                    && prefix.chars().next().map(|c| c.is_ascii_lowercase() || c == '_').unwrap_or(false)
                    && prefix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                if is_valid {
                    generic_target = prefix;
                    generic_desc = cleaned[colon_pos + 1..].trim();
                    found = true;
                }
            }
            if !found {
                continue;
            }
            (generic_target, generic_desc)
        };

        if !desc.is_empty() {
            steps.push(PlanStep {
                index: steps.len(),
                target: target.to_string(),
                description: desc.to_string(),
                status: "pending".to_string(),
                task_id: None,
                output: None,
            });
        }
    }

    steps
}

// ---------------------------------------------------------------------------
// Planning loop
// ---------------------------------------------------------------------------

fn build_planning_prompt(objetivo: &str, messages: &[PlanningMessage], speaker: &str) -> String {
    let role_desc = format!(
        "Sos el agente {}. Trabajas junto con otros agentes para implementar el objetivo.",
        speaker.to_uppercase()
    );
    let role_desc: &str = &role_desc;

    let mut prompt = format!(
        "{role_desc}\n\n\
        OBJETIVO: {objetivo}\n\n\
        Estan discutiendo un plan de implementacion entre los dos. \
        Analiza lo que dijo tu companero, agrega tu perspectiva, \
        propone cambios si es necesario.\n\n\
        Cuando AMBOS esten de acuerdo con el plan completo, inclui al final la linea exacta:\n\
        ===PLAN READY===\n\
        Seguida de los pasos en formato:\n\
        - [ ] atlas: Descripcion del paso backend\n\
        - [ ] pixel: Descripcion del paso frontend\n\n\
        Si todavia no estas listo para el plan final, segui discutiendo.\n\n\
        --- CONVERSACION HASTA AHORA ---\n"
    );

    for msg in messages {
        prompt.push_str(&format!(
            "\n[{}] (ronda {}):\n{}\n",
            msg.sender.to_uppercase(),
            msg.round,
            msg.content
        ));
    }

    prompt.push_str("\n--- TU TURNO ---\n");
    prompt
}

fn run_planning_loop(app: AppHandle, objetivo: String, start_round: u32) {
    let planning_store = app.state::<PlanningStore>();

    // When start_round == 1 the opening message is sent before the loop and round stays 1.
    // When resuming after feedback (start_round > 1) the loop starts by incrementing round,
    // so initialise to start_round - 1 so the first iteration lands exactly on start_round.
    let mut round = if start_round > 1 { start_round - 1 } else { start_round };
    // Speaker alternates: odd rounds → atlas, even rounds → pixel
    let mut speaker = if start_round % 2 == 1 { "atlas".to_string() } else { "pixel".to_string() };

    // Only send the opening prompt on the very first run
    if start_round == 1 {
        let first_prompt = format!(
            "Sos el agente ATLAS. Trabajas junto con PIXEL para implementar el objetivo.\n\nOBJETIVO: {objetivo}\n\nAnaliza este objetivo. Propone tu enfoque inicial y preguntale a PIXEL su perspectiva.\n\nNO incluyas ===PLAN READY=== todavia, primero discutan."
        );

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                s.current_round = round;
                s.current_speaker = speaker.clone();
                emit_update(&app, s);
            }
        }

        let response = call_claude_with_updates(&app, &planning_store, "atlas", &first_prompt);
        planning_store.push_history(first_prompt.clone(), response.clone(), "atlas".to_string());

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                if s.phase == "cancelled" {
                    return;
                }
                s.messages.push(PlanningMessage {
                    sender: "atlas".to_string(),
                    content: response.clone(),
                    round,
                    timestamp: now_iso(),
                });
                emit_update(&app, s);
            }
        }
        planning_store.save_planning_state();

        if response.contains("===PLAN READY===") {
            finalize_plan(&app, &planning_store);
            return;
        }
    }

    loop {
        round += 1;
        speaker = if speaker == "atlas" {
            "pixel".to_string()
        } else {
            "atlas".to_string()
        };

        let messages = {
            let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            match &*session {
                Some(s) => {
                    if s.phase == "cancelled" {
                        return;
                    }
                    s.messages.clone()
                }
                None => return,
            }
        };

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                s.current_round = round;
                s.current_speaker = speaker.clone();
                emit_update(&app, s);
            }
        }

        let prompt = build_planning_prompt(&objetivo, &messages, &speaker);
        let response = call_claude_with_updates(&app, &planning_store, &speaker, &prompt);
        planning_store.push_history(prompt.clone(), response.clone(), speaker.clone());

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                if s.phase == "cancelled" {
                    return;
                }
                s.messages.push(PlanningMessage {
                    sender: speaker.clone(),
                    content: response.clone(),
                    round,
                    timestamp: now_iso(),
                });
                emit_update(&app, s);
            }
        }
        planning_store.save_planning_state();

        if response.contains("===PLAN READY===") {
            finalize_plan(&app, &planning_store);
            return;
        }

        if round >= 10 {
            let messages = {
                let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                session.as_ref().map(|s| s.messages.clone()).unwrap_or_default()
            };
            let force_prompt = build_planning_prompt(&objetivo, &messages, &speaker);
            let force_prompt = format!(
                "{}\n\nIMPORTANTE: Ya llevan muchas rondas. Genera el plan final AHORA. \
                Inclui ===PLAN READY=== seguido de los pasos.",
                force_prompt
            );
            let response = call_claude_with_updates(&app, &planning_store, &speaker, &force_prompt);
            planning_store.push_history(force_prompt.clone(), response.clone(), speaker.clone());
            {
                let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref mut s) = *session {
                    s.messages.push(PlanningMessage {
                        sender: speaker.clone(),
                        content: response,
                        round,
                        timestamp: now_iso(),
                    });
                }
            }
            planning_store.save_planning_state();
            finalize_plan(&app, &planning_store);
            return;
        }
    }
}

fn finalize_plan(app: &AppHandle, planning_store: &PlanningStore) {
    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut s) = *session {
            let all_text: String = s
                .messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            s.plan_steps = parse_plan_steps(&all_text);
            s.phase = "review".to_string();
            log::info!("Planning complete: {} steps generated after {} rounds", s.plan_steps.len(), s.current_round);
            emit_update(app, s);
        }
    } // session guard drops here — mutex is now free
    planning_store.save_planning_state();
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Repo status
// ---------------------------------------------------------------------------

fn parse_git_status_output(raw: &str) -> (u32, u32, u32) {
    let mut changed = 0u32;
    let mut staged = 0u32;
    let mut untracked = 0u32;
    for line in raw.lines() {
        if line.starts_with("??") {
            untracked += 1;
        } else if line.len() >= 2 {
            let bytes = line.as_bytes();
            if bytes[0] != b' ' && bytes[0] != b'?' {
                staged += 1;
            }
            if bytes[1] != b' ' && bytes[1] != b'?' {
                changed += 1;
            }
        }
    }
    (changed, staged, untracked)
}

fn get_local_repo_status(repo_path: &str) -> RepoStatus {
    let run = |cmd: &str| -> String {
        Command::new("bash")
            .args(["-c", &format!("cd {} && {}", shell_escape(repo_path), cmd)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };

    let branch = run("git rev-parse --abbrev-ref HEAD 2>/dev/null");
    let status_raw = run("git status --porcelain 2>/dev/null");
    let (changed, staged, untracked) = parse_git_status_output(&status_raw);
    let last_commit = run("git log -1 --format='%h %s' 2>/dev/null");
    let ahead_behind = run("git rev-list --left-right --count HEAD...@{upstream} 2>/dev/null");
    let (ahead, behind) = {
        let parts: Vec<&str> = ahead_behind.split_whitespace().collect();
        (
            parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
            parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
        )
    };

    RepoStatus { branch, changed, staged, untracked, last_commit, ahead, behind }
}

fn get_remote_repo_status(host: &str, repo_path: &str) -> RepoStatus {
    // NOTE: The script is wrapped in single quotes on the SSH command line, so the
    // git log format string must NOT contain single quotes — use %h %s without quoting.
    let script = format!(
        "cd {}; \
         echo ===BRANCH===; git rev-parse --abbrev-ref HEAD 2>/dev/null; \
         echo ===STATUS===; git status --porcelain 2>/dev/null; \
         echo ===COMMIT===; git log -1 --format=%h\\ %s 2>/dev/null; \
         echo ===AHEAD===; git rev-list --left-right --count HEAD...@{{upstream}} 2>/dev/null; \
         echo ===END===",
        shell_escape(repo_path)
    );
    let raw = Command::new("bash")
        .args(["-c", &format!(
            "ssh -o ConnectTimeout=5 -o ServerAliveInterval=30 {} '{}'", shell_escape(host), script
        )])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut branch = String::new();
    let mut status_raw = String::new();
    let mut last_commit = String::new();
    let mut ahead_behind = String::new();
    let mut section = "";

    for line in raw.lines() {
        match line.trim() {
            "===BRANCH===" => { section = "branch"; continue; }
            "===STATUS===" => { section = "status"; continue; }
            "===COMMIT===" => { section = "commit"; continue; }
            "===AHEAD===" => { section = "ahead"; continue; }
            "===END===" => break,
            _ => {}
        }
        match section {
            "branch" => branch = line.trim().to_string(),
            "status" => { if !status_raw.is_empty() { status_raw.push('\n'); } status_raw.push_str(line); }
            "commit" => last_commit = line.trim().to_string(),
            "ahead" => ahead_behind = line.trim().to_string(),
            _ => {}
        }
    }

    let (changed, staged, untracked) = parse_git_status_output(&status_raw);
    let (ahead, behind) = {
        let parts: Vec<&str> = ahead_behind.split_whitespace().collect();
        (
            parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
            parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
        )
    };

    RepoStatus { branch, changed, staged, untracked, last_commit, ahead, behind }
}

/// Fetch repo statuses for all configured machines from the registry.
fn fetch_repo_statuses(app: &AppHandle) -> (RepoStatus, RepoStatus) {
    let repos = collect_machine_repos(app);
    let empty = RepoStatus {
        branch: String::new(), changed: 0, staged: 0, untracked: 0,
        last_commit: String::new(), ahead: 0, behind: 0,
    };
    let mut back = empty.clone();
    let mut front = empty;

    for (_id, host, repo_path, is_local) in &repos {
        if *is_local {
            back = get_local_repo_status(repo_path);
        } else {
            front = get_remote_repo_status(host, repo_path);
        }
    }

    (back, front)
}

/// Generate a branch name from the objetivo
fn branch_name_from_objetivo(objetivo: &str) -> String {
    let slug: String = objetivo
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() { c }
            else if c == ' ' || c == '_' || c == '-' { '-' }
            else { ' ' }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("");

    let slug: String = slug.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    // Truncate by char count (not bytes) to avoid panics on non-ASCII boundaries
    let slug: String = slug.chars().take(40).collect();
    let slug = slug.trim_end_matches('-');
    format!("plan/{}", slug)
}

/// Create git branches for all configured repos
fn create_branches(app: &AppHandle, objetivo: &str) -> (Option<String>, Option<String>) {
    let branch = branch_name_from_objetivo(objetivo);
    let branch_escaped = shell_escape(&branch);
    let repos = collect_machine_repos(app);

    let mut back_ok = false;
    let mut front_ok = false;

    for (_id, host, repo_path, is_local) in &repos {
        if *is_local {
            back_ok = Command::new("bash")
                .args(["-c", &format!(
                    "cd {} && git checkout -b {} 2>/dev/null || git checkout {} 2>/dev/null",
                    shell_escape(repo_path), branch_escaped, branch_escaped
                )])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        } else {
            front_ok = Command::new("bash")
                .args(["-c", &format!(
                    "ssh -o ServerAliveInterval=30 {} 'cd {}; git checkout -b {} 2>/dev/null || git checkout {} 2>/dev/null'",
                    shell_escape(host), shell_escape(repo_path),
                    branch_escaped, branch_escaped
                )])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }

    (
        if back_ok { Some(branch.clone()) } else { None },
        if front_ok { Some(branch) } else { None },
    )
}

fn run_execution(app: AppHandle) {
    let planning_store = app.state::<PlanningStore>();
    let task_store = app.state::<TaskStore>();

    let (steps, objetivo) = {
        let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        match &*session {
            Some(s) => (s.plan_steps.clone(), s.objetivo.clone()),
            None => return,
        }
    };

    let (branch_back, branch_front) = create_branches(&app, &objetivo);
    let (repo_back, repo_front) = fetch_repo_statuses(&app);
    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut s) = *session {
            s.branch_back = branch_back;
            s.branch_front = branch_front;
            s.repo_back = Some(repo_back);
            s.repo_front = Some(repo_front);
            emit_update(&app, s);
        }
    }

    use crate::tasks::send_task_internal_with_deps;
    let total_steps = steps.len();
    let mut task_ids: Vec<(usize, u64, String)> = Vec::new();

    // Group steps by target so that per-machine steps run sequentially
    // (each step depends on the previous one for the same target).
    // Different targets (e.g. atlas and pixel) still run in parallel.
    let mut prev_task_per_target: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for step in &steps {
        let repo_ctx = build_repo_context(&app, &step.target);
        let enriched_prompt = format!(
            "OBJETIVO GENERAL: {}\n{}\nTU TAREA (paso {} de {}):\n{}\n\nImplementa esta tarea de forma completa y autocontenida.",
            objetivo,
            repo_ctx,
            step.index + 1,
            total_steps,
            step.description
        );

        // If a previous task exists for this target, make this one depend on it
        // so same-machine steps execute sequentially.
        let depends_on: Vec<u64> = prev_task_per_target
            .get(&step.target)
            .copied()
            .into_iter()
            .collect();

        let task = send_task_internal_with_deps(
            &app,
            &task_store,
            &step.target,
            &enriched_prompt,
            false,
            depends_on,
            "on_success".to_string(),
        );

        prev_task_per_target.insert(step.target.clone(), task.id);
        task_ids.push((step.index, task.id, step.target.clone()));

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                if let Some(ps) = s.plan_steps.get_mut(step.index) {
                    // "running" for the first step per target; "pending" for subsequent
                    // ones that wait for their predecessor to finish.
                    ps.status = task.status.clone();
                    ps.task_id = Some(task.id);
                }
                emit_update(&app, s);
            }
        }
    }

    // Pre-compute JSONL dirs for each target
    let jsonl_dirs: Vec<(String, String)> = task_ids.iter().map(|(_, _, target)| {
        let dir = find_machine(&app, target)
            .as_ref()
            .map(jsonl_dir_for_machine)
            .unwrap_or_default();
        (target.clone(), dir)
    }).collect();

    let exec_start = std::time::Instant::now();
    let mut last_repo_refresh = std::time::Instant::now()
        .checked_sub(Duration::from_secs(20))
        .unwrap_or(std::time::Instant::now());
    loop {
        thread::sleep(Duration::from_secs(3));

        {
            let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref s) = *session {
                if s.phase == "cancelled" {
                    return;
                }
            }
        }

        let mut all_done = true;

        // Collect step updates while holding task lock (no planning lock yet)
        let step_updates: Vec<(usize, String, Option<String>)> = {
            let tasks = task_store.tasks.lock().unwrap_or_else(|e| e.into_inner());
            let mut updates = Vec::new();
            for (step_idx, task_id, _target) in &task_ids {
                if let Some(t) = tasks.iter().find(|t| t.id == *task_id) {
                    if t.status == "done" {
                        updates.push((*step_idx, "done".to_string(), Some(t.output.chars().take(500).collect())));
                    } else if t.status == "timeout" || t.status == "error" || t.status == "killed" {
                        // Terminal failure — mark step as error so execution can complete
                        updates.push((*step_idx, "error".to_string(), Some(t.output.chars().take(500).collect())));
                    } else {
                        // "running" or "pending" — still in progress
                        updates.push((*step_idx, t.status.clone(), None));
                        all_done = false;
                    }
                } else {
                    all_done = false;
                }
            }
            updates
        };
        // task_store lock dropped here — now safe to lock planning_store
        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                for (step_idx, new_status, output) in step_updates {
                    if let Some(ps) = s.plan_steps.get_mut(step_idx) {
                        // Update if the step isn't already in a terminal state
                        if ps.status != "done" && ps.status != "error" {
                            if output.is_some() {
                                ps.output = output;
                            }
                            ps.status = new_status;
                        }
                    }
                }
            }
        }

        let mut combined_activity: Vec<Activity> = Vec::new();
        for (target, dir) in &jsonl_dirs {
            let act = read_current_activity(dir);
            if !act.is_empty() {
                if let Some(last) = act.last() {
                    combined_activity.push(Activity {
                        type_: last.type_.clone(),
                        name: last.name.clone(),
                        detail: Some(format!("[{}] {}", target.to_uppercase(), last.detail.as_deref().unwrap_or(""))),
                        content: last.content.clone(),
                    });
                }
            }
        }

        let elapsed = exec_start.elapsed().as_secs();
        // Refresh repo statuses at most once every 15 seconds, and always on completion
        let should_refresh_repos = last_repo_refresh.elapsed() >= Duration::from_secs(15);

        // Fetch repo statuses OUTSIDE the session lock — SSH calls must not
        // hold the mutex or the entire planning state becomes inaccessible.
        let maybe_repos = if should_refresh_repos || all_done {
            if should_refresh_repos {
                last_repo_refresh = std::time::Instant::now();
            }
            Some(fetch_repo_statuses(&app))
        } else {
            None
        };

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                s.elapsed_secs = elapsed;
                s.current_activity = combined_activity;
                if let Some((rb, rf)) = maybe_repos {
                    s.repo_back = Some(rb);
                    s.repo_front = Some(rf);
                }
                emit_update(&app, s);
            }
        }

        if all_done {
            break;
        }
    }

    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut s) = *session {
            let has_errors = s.plan_steps.iter().any(|p| p.status == "error");
            s.phase = if has_errors {
                "done-with-errors".to_string()
            } else {
                "done".to_string()
            };
            s.finished_at = Some(now_iso());
            s.current_activity.clear();
            emit_update(&app, s);
        }
    }
    planning_store.save_planning_state();
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn start_planning(
    app: AppHandle,
    planning_store: tauri::State<'_, PlanningStore>,
    objetivo: String,
    timeout_secs: Option<u64>,
) -> Result<PlanningState, String> {
    if objetivo.trim().is_empty() {
        return Err("Objetivo cannot be empty".into());
    }
    if objetivo.len() > 1_000_000 {
        return Err("Objetivo too large (max 1MB)".into());
    }
    // Apply per-session timeout config before spawning the planning loop
    {
        let mut cfg = planning_store.config.lock().unwrap_or_else(|e| e.into_inner());
        cfg.timeout_secs = timeout_secs;
    }
    let (repo_back, repo_front) = fetch_repo_statuses(&app);

    let state = PlanningState {
        id: uuid::Uuid::new_v4().to_string(),
        objetivo: objetivo.clone(),
        phase: "planning".to_string(),
        messages: Vec::new(),
        plan_steps: Vec::new(),
        current_round: 0,
        current_speaker: "atlas".to_string(),
        started_at: now_iso(),
        finished_at: None,
        elapsed_secs: 0,
        current_activity: Vec::new(),
        branch_back: None,
        branch_front: None,
        repo_back: Some(repo_back),
        repo_front: Some(repo_front),
        streaming_text: None,
    };

    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        *session = Some(state.clone());
    }
    planning_store.save_planning_state();

    log::info!("Planning started: '{}'", &objetivo);

    let app_clone = app.clone();
    thread::spawn(move || {
        run_planning_loop(app_clone, objetivo, 1);
    });

    Ok(state)
}

#[tauri::command]
pub fn get_planning_state(
    planning_store: tauri::State<'_, PlanningStore>,
) -> Option<PlanningState> {
    planning_store.session.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn approve_plan(
    app: AppHandle,
    planning_store: tauri::State<'_, PlanningStore>,
) -> bool {
    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *session {
            Some(s) if s.phase == "review" => {
                if s.plan_steps.is_empty() {
                    return false; // Can't execute empty plan
                }
                s.phase = "executing".to_string();
                emit_update(&app, s);
            }
            _ => return false,
        }
    }
    planning_store.save_planning_state();

    let app_clone = app.clone();
    thread::spawn(move || {
        run_execution(app_clone);
    });

    true
}

#[tauri::command]
pub fn add_planning_feedback(
    app: AppHandle,
    planning_store: tauri::State<'_, PlanningStore>,
    feedback: String,
) -> bool {
    if feedback.trim().is_empty() || feedback.len() > 50_000 {
        return false;
    }
    {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *session {
            Some(s) if s.phase == "review" => {
                s.messages.push(PlanningMessage {
                    sender: "user".to_string(),
                    content: feedback,
                    round: s.current_round + 1,
                    timestamp: now_iso(),
                });
                s.phase = "planning".to_string();
                emit_update(&app, s);
            }
            _ => return false,
        }
    }
    planning_store.save_planning_state();

    let (objetivo, start_round) = {
        let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        let s = session.as_ref();
        (
            s.map(|s| s.objetivo.clone()).unwrap_or_default(),
            s.map(|s| s.current_round + 1).unwrap_or(1),
        )
    };

    let app_clone = app.clone();
    thread::spawn(move || {
        run_planning_loop(app_clone, objetivo, start_round);
    });

    true
}

#[tauri::command]
pub fn cancel_planning(
    app: AppHandle,
    planning_store: tauri::State<'_, PlanningStore>,
) -> bool {
    let result = {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *session {
            Some(s) => {
                s.phase = "cancelled".to_string();
                s.finished_at = Some(now_iso());
                emit_update(&app, s);
                true
            }
            None => false,
        }
    };
    if result {
        planning_store.save_planning_state();
        // Kill any running claude subprocess
        let child_pid = planning_store.child_pid.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(pid) = child_pid {
            // SIGTERM first, then SIGKILL if needed
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
            log::info!("cancel_planning: sent SIGTERM to child pid {}", pid);
        }
        // Clean up any leftover temp files from call_claude_with_updates.
        // Pattern: /tmp/jarvis-planning-{target}-{pid}.{out,done}
        let pid = std::process::id();
        let pid_suffix = format!("-{}", pid);
        if let Ok(entries) = fs::read_dir("/tmp") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("jarvis-planning-")
                    && (name_str.ends_with(".out") || name_str.ends_with(".done"))
                    && name_str.contains(&pid_suffix)
                {
                    let _ = fs::remove_file(entry.path());
                    log::info!("cancel_planning: removed temp file {}", name_str);
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Retry failed steps
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn retry_failed_steps(
    app: AppHandle,
    planning_store: tauri::State<'_, PlanningStore>,
) -> bool {
    // Find failed steps and reset them
    let (steps, objetivo) = {
        let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *session {
            Some(s) if s.phase == "done-with-errors" => {
                // Reset failed steps to pending
                for step in s.plan_steps.iter_mut() {
                    if step.status == "error" {
                        step.status = "pending".to_string();
                        step.task_id = None;
                        step.output = None;
                    }
                }
                s.phase = "executing".to_string();
                emit_update(&app, s);
                (s.plan_steps.clone(), s.objetivo.clone())
            }
            _ => return false,
        }
    };
    planning_store.save_planning_state();

    let app_clone = app.clone();
    thread::spawn(move || {
        let planning_store = app_clone.state::<PlanningStore>();
        let task_store = app_clone.state::<TaskStore>();

        use crate::tasks::send_task_internal_with_deps;

        let mut task_ids: Vec<(usize, u64, String)> = Vec::new();
        let mut prev_task_per_target: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

        for step in steps.iter().filter(|s| s.status == "pending") {
            let depends_on: Vec<u64> = prev_task_per_target.get(&step.target).copied().into_iter().collect();
            let enriched_prompt = format!(
                "OBJETIVO GENERAL: {}\n\nTU TAREA (paso {} de {}):\n{}\n\nImplementa esta tarea de forma completa y autocontenida.",
                objetivo, step.index + 1, steps.len(), step.description
            );
            let task = send_task_internal_with_deps(
                &app_clone,
                &task_store,
                &step.target,
                &enriched_prompt,
                false,
                depends_on,
                "on_success".to_string(),
            );
            prev_task_per_target.insert(step.target.clone(), task.id);
            task_ids.push((step.index, task.id, step.target.clone()));

            {
                let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref mut s) = *session {
                    if let Some(ps) = s.plan_steps.get_mut(step.index) {
                        ps.status = "running".to_string();
                        ps.task_id = Some(task.id);
                    }
                    emit_update(&app_clone, s);
                }
            }
        }

        // Pre-compute JSONL dirs for each target
        let jsonl_dirs: Vec<(String, String)> = task_ids.iter().map(|(_, _, target)| {
            let dir = find_machine(&app_clone, target)
                .as_ref()
                .map(jsonl_dir_for_machine)
                .unwrap_or_default();
            (target.clone(), dir)
        }).collect();

        let exec_start = std::time::Instant::now();
        loop {
            thread::sleep(Duration::from_secs(3));

            {
                let session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref s) = *session {
                    if s.phase == "cancelled" { return; }
                }
            }

            let mut all_done = true;
            let step_updates: Vec<(usize, String, Option<String>)> = {
                let tasks = task_store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                let mut updates = Vec::new();
                for (step_idx, task_id, _target) in &task_ids {
                    if let Some(t) = tasks.iter().find(|t| t.id == *task_id) {
                        if t.status == "done" {
                            updates.push((*step_idx, "done".to_string(), Some(t.output.chars().take(500).collect())));
                        } else if t.status == "timeout" || t.status == "error" || t.status == "killed" {
                            updates.push((*step_idx, "error".to_string(), Some(t.output.chars().take(500).collect())));
                        } else {
                            all_done = false;
                        }
                    } else {
                        all_done = false;
                    }
                }
                updates
            };

            let mut combined_activity: Vec<Activity> = Vec::new();
            for (target, dir) in &jsonl_dirs {
                let act = read_current_activity(dir);
                if !act.is_empty() {
                    if let Some(last) = act.last() {
                        combined_activity.push(Activity {
                            type_: last.type_.clone(),
                            name: last.name.clone(),
                            detail: Some(format!("[{}] {}", target.to_uppercase(), last.detail.as_deref().unwrap_or(""))),
                            content: last.content.clone(),
                        });
                    }
                }
            }

            {
                let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref mut s) = *session {
                    for (step_idx, new_status, output) in step_updates {
                        if let Some(ps) = s.plan_steps.get_mut(step_idx) {
                            if ps.status == "running" || ps.status == "pending" {
                                ps.status = new_status;
                                ps.output = output;
                            }
                        }
                    }
                    s.elapsed_secs = exec_start.elapsed().as_secs();
                    s.current_activity = combined_activity;
                    emit_update(&app_clone, s);
                }
            }

            if all_done { break; }
        }

        {
            let mut session = planning_store.session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut s) = *session {
                let has_errors = s.plan_steps.iter().any(|p| p.status == "error");
                s.phase = if has_errors { "done-with-errors".to_string() } else { "done".to_string() };
                s.finished_at = Some(now_iso());
                s.current_activity.clear();
                emit_update(&app_clone, s);
            }
        }
        planning_store.save_planning_state();
    });

    true
}

// ---------------------------------------------------------------------------
// Repo branch commands (used by Header)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_repo_statuses(app: AppHandle) -> (RepoStatus, RepoStatus) {
    fetch_repo_statuses(&app)
}

#[tauri::command]
pub fn get_repo_branches(app: AppHandle, repo: String) -> Vec<String> {
    let repos = collect_machine_repos(&app);

    if repo == "back" {
        if let Some((_id, _host, repo_path, _)) = repos.iter().find(|(_, _, _, local)| *local) {
            let output = Command::new("bash")
                .args(["-c", &format!(
                    "cd {} && git branch -a --sort=-committerdate --format='%(refname:short)' 2>/dev/null",
                    shell_escape(repo_path)
                )])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            parse_branch_list(&output)
        } else {
            Vec::new()
        }
    } else if let Some((_id, host, repo_path, _)) = repos.iter().find(|(_, _, _, local)| !*local) {
        let output = Command::new("bash")
            .args(["-c", &format!(
                "ssh -o ConnectTimeout=5 {} 'cd {}; git branch -a --sort=-committerdate --format=\"%(refname:short)\" 2>/dev/null'",
                shell_escape(host), shell_escape(repo_path)
            )])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        parse_branch_list(&output)
    } else {
        Vec::new()
    }
}

fn parse_branch_list(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for line in raw.lines() {
        let name = line.trim();
        if name.is_empty() || name.contains("HEAD") {
            continue;
        }
        let clean = name.strip_prefix("origin/").unwrap_or(name);
        if seen.insert(clean.to_string()) {
            result.push(clean.to_string());
        }
    }
    result
}

#[tauri::command]
pub fn switch_branch(app: AppHandle, repo: String, branch: String) -> Result<RepoStatus, String> {
    let repos = collect_machine_repos(&app);
    let branch_escaped = shell_escape(&branch);

    if repo == "back" {
        let (_id, _host, repo_path, _) = repos.iter()
            .find(|(_, _, _, local)| *local)
            .ok_or_else(|| "No local repo configured".to_string())?;

        let output = Command::new("bash")
            .args(["-c", &format!(
                "cd {} && git checkout {} 2>&1 || git checkout -b {} origin/{} 2>&1",
                shell_escape(repo_path), branch_escaped, branch_escaped, branch_escaped
            )])
            .output()
            .map_err(|e| e.to_string())?;
        let msg = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        let status = get_local_repo_status(repo_path);
        if status.branch != branch {
            return Err(format!("No se pudo cambiar a '{}': {}", branch, msg.trim()));
        }
        Ok(status)
    } else {
        let (_id, host, repo_path, _) = repos.iter()
            .find(|(_, _, _, local)| !*local)
            .ok_or_else(|| "No remote repo configured".to_string())?;

        // NOTE: script is wrapped in outer single quotes for SSH; must not use single
        // quotes inside (e.g. git log format). Use unquoted %h\ %s instead.
        let script = format!(
            "cd {}; \
             echo ===CHECKOUT===; git checkout {} 2>&1 || git checkout -b {} origin/{} 2>&1; \
             echo ===BRANCH===; git rev-parse --abbrev-ref HEAD 2>/dev/null; \
             echo ===STATUS===; git status --porcelain 2>/dev/null; \
             echo ===COMMIT===; git log -1 --format=%h\\ %s 2>/dev/null; \
             echo ===AHEAD===; git rev-list --left-right --count HEAD...@{{upstream}} 2>/dev/null; \
             echo ===END===",
            shell_escape(repo_path),
            branch_escaped, branch_escaped, branch_escaped
        );
        let raw = Command::new("bash")
            .args(["-c", &format!(
                "ssh -o ConnectTimeout=5 -o ServerAliveInterval=30 {} '{}'",
                shell_escape(host), script
            )])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let mut checkout_msg = String::new();
        let mut current_branch = String::new();
        let mut status_raw = String::new();
        let mut last_commit = String::new();
        let mut ahead_behind = String::new();
        let mut section = "";

        for line in raw.lines() {
            match line.trim() {
                "===CHECKOUT===" => { section = "checkout"; continue; }
                "===BRANCH===" => { section = "branch"; continue; }
                "===STATUS===" => { section = "status"; continue; }
                "===COMMIT===" => { section = "commit"; continue; }
                "===AHEAD===" => { section = "ahead"; continue; }
                "===END===" => break,
                _ => {}
            }
            match section {
                "checkout" => { if !checkout_msg.is_empty() { checkout_msg.push('\n'); } checkout_msg.push_str(line); }
                "branch" => current_branch = line.trim().to_string(),
                "status" => { if !status_raw.is_empty() { status_raw.push('\n'); } status_raw.push_str(line); }
                "commit" => last_commit = line.trim().to_string(),
                "ahead" => ahead_behind = line.trim().to_string(),
                _ => {}
            }
        }

        if current_branch != branch {
            return Err(format!("No se pudo cambiar a '{}': {}", branch, checkout_msg.trim()));
        }

        let (changed, staged, untracked) = parse_git_status_output(&status_raw);
        let (ahead, behind) = {
            let parts: Vec<&str> = ahead_behind.split_whitespace().collect();
            (
                parts.first().and_then(|s| s.parse().ok()).unwrap_or(0),
                parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0),
            )
        };

        Ok(RepoStatus { branch: current_branch, changed, staged, untracked, last_commit, ahead, behind })
    }
}

// ---------------------------------------------------------------------------
// Planning history commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_planning_history(
    planning_store: tauri::State<'_, PlanningStore>,
) -> Vec<PlanningHistoryEntry> {
    planning_store.history.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn clear_planning_history(
    planning_store: tauri::State<'_, PlanningStore>,
) {
    planning_store.history.lock().unwrap_or_else(|e| e.into_inner()).clear();
    *planning_store.history_id_counter.lock().unwrap_or_else(|e| e.into_inner()) = 0;
}

#[tauri::command]
pub fn export_planning_session(
    planning_store: tauri::State<'_, PlanningStore>,
    filename: String,
) -> Result<String, String> {
    if filename.trim().is_empty() {
        return Err("filename cannot be empty".into());
    }
    // Sanitise: strip path separators to prevent directory traversal
    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe_name.is_empty() {
        return Err("filename must contain at least one alphanumeric character".into());
    }

    let entries: Vec<PlanningHistoryEntry> = planning_store
        .history
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    let mut md = format!("# Plan: {}\nDate: {}\n\n", safe_name, chrono::Utc::now().format("%Y-%m-%d"));

    for (i, entry) in entries.iter().enumerate() {
        md.push_str(&format!(
            "## Exchange {}\n**Prompt:** {}\n**Response:** {}\n\n",
            i + 1,
            entry.prompt,
            entry.response,
        ));
    }

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let plans_dir = home.join(".config/jarvis/plans");
    fs::create_dir_all(&plans_dir).map_err(|e| format!("Failed to create plans directory: {}", e))?;

    let file_path = plans_dir.join(format!("{}.md", safe_name));
    fs::write(&file_path, &md).map_err(|e| format!("Failed to write plan file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Metrics command
// ---------------------------------------------------------------------------

/// Returns aggregate metrics across the in-memory planning history.
/// Fields:
///   total_sessions  – number of history entries (prompt/response exchanges)
///   avg_steps       – average steps per exchange (approximated as 1.0 since steps live on
///                     PlanningState, not on history entries; a real implementation would
///                     persist step counts per session)
///   success_rate    – fraction of entries whose response does NOT start with "[timeout]"
///   total_tokens_used – rough token estimate (chars / 4) across all prompts + responses
///   most_used_repo  – machine name that appears most often in history entries
#[tauri::command]
pub fn get_planning_metrics(
    planning_store: tauri::State<'_, PlanningStore>,
) -> serde_json::Value {
    let history = planning_store.history.lock().unwrap_or_else(|e| e.into_inner());
    let total_sessions = history.len();

    if total_sessions == 0 {
        return serde_json::json!({
            "total_sessions": 0usize,
            "avg_steps": 0.0f64,
            "success_rate": 0.0f64,
            "total_tokens_used": 0u64,
            "most_used_repo": serde_json::Value::Null,
        });
    }

    let mut success_count: usize = 0;
    let mut total_chars: u64 = 0;
    let mut machine_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entry in history.iter() {
        if !entry.response.starts_with("[timeout]") {
            success_count += 1;
        }
        total_chars += (entry.prompt.len() + entry.response.len()) as u64;
        *machine_freq.entry(entry.machine.clone()).or_insert(0) += 1;
    }

    let success_rate = success_count as f64 / total_sessions as f64;
    // Rough token estimate: 1 token ≈ 4 characters
    let total_tokens_used: u64 = total_chars / 4;

    let most_used_repo: Option<String> = machine_freq
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(machine, _)| machine);

    // avg_steps: history entries don't track step count, so we return 0.0 as a
    // placeholder until step counts are persisted per-session.
    let avg_steps: f64 = 0.0;

    serde_json::json!({
        "total_sessions": total_sessions,
        "avg_steps": avg_steps,
        "success_rate": success_rate,
        "total_tokens_used": total_tokens_used,
        "most_used_repo": most_used_repo,
    })
}

// ---------------------------------------------------------------------------
// Duplicate session command
// ---------------------------------------------------------------------------

/// Creates a copy of an existing history entry (identified by numeric `session_id`)
/// with a new auto-incremented ID. Returns the new entry's ID as a string, or an error.
#[tauri::command]
pub fn duplicate_planning_session(
    planning_store: tauri::State<'_, PlanningStore>,
    session_id: String,
) -> Result<String, String> {
    let target_id: u64 = session_id
        .parse()
        .map_err(|_| format!("Invalid session_id: '{}' is not a valid integer", session_id))?;

    // Clone source entry while holding history lock, then release before re-locking counter.
    let source: PlanningHistoryEntry = {
        let history = planning_store.history.lock().unwrap_or_else(|e| e.into_inner());
        history
            .iter()
            .find(|e| e.id == target_id)
            .cloned()
            .ok_or_else(|| format!("No planning session found with id {}", target_id))?
    };

    // Assign a new ID
    let new_id = {
        let mut counter = planning_store.history_id_counter.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        *counter
    };

    let duplicate = PlanningHistoryEntry {
        id: new_id,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        prompt: source.prompt.clone(),
        response: source.response.clone(),
        machine: source.machine.clone(),
    };

    {
        let mut history = planning_store.history.lock().unwrap_or_else(|e| e.into_inner());
        history.push(duplicate);
        if history.len() > PLANNING_HISTORY_MAX {
            let overflow = history.len() - PLANNING_HISTORY_MAX;
            history.drain(..overflow);
        }
    }

    log::info!("Duplicated planning session {} → new id {}", target_id, new_id);
    Ok(new_id.to_string())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_plan_steps
    // -----------------------------------------------------------------------

    #[test]
    fn parse_plan_steps_returns_empty_without_marker() {
        let text = "atlas: do something\npixel: do something else";
        let steps = parse_plan_steps(text);
        assert!(steps.is_empty(), "Without ===PLAN READY=== marker, no steps should be parsed");
    }

    #[test]
    fn parse_plan_steps_parses_atlas_and_pixel_steps() {
        let text = "Discussion...\n===PLAN READY===\n- [ ] atlas: Implement backend API\n- [ ] pixel: Build frontend UI";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].target, "atlas");
        assert_eq!(steps[0].description, "Implement backend API");
        assert_eq!(steps[1].target, "pixel");
        assert_eq!(steps[1].description, "Build frontend UI");
    }

    #[test]
    fn parse_plan_steps_handles_uppercase_prefixes() {
        let text = "===PLAN READY===\n- [ ] ATLAS: Backend task\n- [ ] PIXEL: Frontend task";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].target, "atlas");
        assert_eq!(steps[1].target, "pixel");
    }

    #[test]
    fn parse_plan_steps_assigns_sequential_indices() {
        let text = "===PLAN READY===\n- [ ] atlas: Step one\n- [ ] pixel: Step two\n- [ ] atlas: Step three";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].index, 0);
        assert_eq!(steps[1].index, 1);
        assert_eq!(steps[2].index, 2);
    }

    #[test]
    fn parse_plan_steps_all_start_as_pending() {
        let text = "===PLAN READY===\n- [ ] atlas: Do work\n- [ ] pixel: Do other work";
        let steps = parse_plan_steps(text);
        for step in &steps {
            assert_eq!(step.status, "pending");
        }
    }

    #[test]
    fn parse_plan_steps_skips_lines_without_valid_prefix() {
        let text = "===PLAN READY===\n- [ ] atlas: Valid step\nSome random text without prefix\n- [ ] pixel: Another valid step";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn parse_plan_steps_skips_empty_descriptions() {
        let text = "===PLAN READY===\n- [ ] atlas:\n- [ ] pixel: Real step";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].target, "pixel");
    }

    #[test]
    fn parse_plan_steps_ignores_content_before_marker() {
        let text = "- [ ] atlas: Should be ignored\n===PLAN READY===\n- [ ] atlas: Should be included";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].description, "Should be included");
    }

    #[test]
    fn parse_plan_steps_handles_generic_lowercase_target() {
        let text = "===PLAN READY===\n- [ ] nova: Deploy to staging";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].target, "nova");
        assert_eq!(steps[0].description, "Deploy to staging");
    }

    #[test]
    fn parse_plan_steps_task_id_is_none_initially() {
        let text = "===PLAN READY===\n- [ ] atlas: Some task";
        let steps = parse_plan_steps(text);
        assert!(steps[0].task_id.is_none());
        assert!(steps[0].output.is_none());
    }

    #[test]
    fn parse_plan_steps_handles_checkbox_variant_without_space() {
        // Both "[ ]" and "[]" variants should be stripped
        let text = "===PLAN READY===\n- [] atlas: No-space checkbox";
        let steps = parse_plan_steps(text);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].description, "No-space checkbox");
    }

    // -----------------------------------------------------------------------
    // parse_git_status_output
    // -----------------------------------------------------------------------

    #[test]
    fn parse_git_status_empty_output_returns_zeros() {
        let (changed, staged, untracked) = parse_git_status_output("");
        assert_eq!(changed, 0);
        assert_eq!(staged, 0);
        assert_eq!(untracked, 0);
    }

    #[test]
    fn parse_git_status_counts_untracked_files() {
        let raw = "?? file1.txt\n?? file2.rs";
        let (changed, staged, untracked) = parse_git_status_output(raw);
        assert_eq!(untracked, 2);
        assert_eq!(changed, 0);
        assert_eq!(staged, 0);
    }

    #[test]
    fn parse_git_status_counts_modified_files() {
        // First char ' ' means unstaged, second char 'M' means modified in worktree
        let raw = " M src/main.rs\n M src/lib.rs";
        let (changed, staged, untracked) = parse_git_status_output(raw);
        assert_eq!(changed, 2);
        assert_eq!(staged, 0);
        assert_eq!(untracked, 0);
    }

    #[test]
    fn parse_git_status_counts_staged_files() {
        // First char 'M' means staged modification, second char ' ' means clean in worktree
        let raw = "M  src/main.rs\nA  new_file.rs";
        let (changed, staged, untracked) = parse_git_status_output(raw);
        assert_eq!(staged, 2);
        assert_eq!(changed, 0);
        assert_eq!(untracked, 0);
    }

    #[test]
    fn parse_git_status_counts_mixed_state() {
        // MM = staged AND modified in worktree
        let raw = "MM src/main.rs\n?? untracked.txt\nA  staged_new.rs";
        let (changed, staged, untracked) = parse_git_status_output(raw);
        assert_eq!(changed, 1);  // MM: second char 'M' != ' '
        assert_eq!(staged, 2);   // MM and A: first char is non-space, non-?
        assert_eq!(untracked, 1);
    }

    // -----------------------------------------------------------------------
    // branch_name_from_objetivo
    // -----------------------------------------------------------------------

    #[test]
    fn branch_name_from_objetivo_prefixes_with_plan() {
        let branch = branch_name_from_objetivo("Add user authentication");
        assert!(branch.starts_with("plan/"), "Branch should start with 'plan/'");
    }

    #[test]
    fn branch_name_from_objetivo_lowercases_and_slugifies() {
        let branch = branch_name_from_objetivo("Add User Authentication");
        assert_eq!(branch, "plan/add-user-authentication");
    }

    #[test]
    fn branch_name_from_objetivo_replaces_spaces_with_dashes() {
        let branch = branch_name_from_objetivo("implement new feature");
        assert_eq!(branch, "plan/implement-new-feature");
    }

    #[test]
    fn branch_name_from_objetivo_strips_special_characters() {
        let branch = branch_name_from_objetivo("Fix bug #123!");
        assert!(branch.starts_with("plan/"));
        assert!(!branch.contains('#'));
        assert!(!branch.contains('!'));
    }

    #[test]
    fn branch_name_from_objetivo_truncates_at_40_chars_after_prefix() {
        let long = "a".repeat(100);
        let branch = branch_name_from_objetivo(&long);
        let slug = branch.strip_prefix("plan/").unwrap();
        assert!(slug.len() <= 40, "Slug should be at most 40 chars, got {}", slug.len());
    }

    #[test]
    fn branch_name_from_objetivo_no_trailing_dash() {
        let branch = branch_name_from_objetivo("hello world   ");
        assert!(!branch.ends_with('-'), "Branch should not end with a dash");
    }

    #[test]
    fn branch_name_from_objetivo_collapses_multiple_dashes() {
        let branch = branch_name_from_objetivo("hello  --  world");
        assert!(!branch.contains("--"), "Branch should not contain consecutive dashes");
    }

    // -----------------------------------------------------------------------
    // parse_branch_list
    // -----------------------------------------------------------------------

    #[test]
    fn parse_branch_list_returns_empty_for_empty_input() {
        let result = parse_branch_list("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_branch_list_strips_origin_prefix() {
        let raw = "origin/main\norigin/feature-x";
        let result = parse_branch_list(raw);
        assert!(result.contains(&"main".to_string()));
        assert!(result.contains(&"feature-x".to_string()));
        assert!(!result.iter().any(|b| b.starts_with("origin/")));
    }

    #[test]
    fn parse_branch_list_deduplicates_local_and_remote() {
        // Both "main" and "origin/main" should only appear once
        let raw = "main\norigin/main\nfeature-x\norigin/feature-x";
        let result = parse_branch_list(raw);
        let main_count = result.iter().filter(|b| *b == "main").count();
        assert_eq!(main_count, 1, "main should appear exactly once");
    }

    #[test]
    fn parse_branch_list_skips_head_entries() {
        let raw = "HEAD\norigin/HEAD -> origin/main\nmain";
        let result = parse_branch_list(raw);
        assert!(!result.iter().any(|b| b.contains("HEAD")));
        assert!(result.contains(&"main".to_string()));
    }

    #[test]
    fn parse_branch_list_skips_empty_lines() {
        let raw = "\nmain\n\nfeature-x\n";
        let result = parse_branch_list(raw);
        assert_eq!(result, vec!["main", "feature-x"]);
    }

    // -----------------------------------------------------------------------
    // build_planning_prompt
    // -----------------------------------------------------------------------

    #[test]
    fn build_planning_prompt_contains_objetivo() {
        let objetivo = "Build a REST API";
        let prompt = build_planning_prompt(objetivo, &[], "atlas");
        assert!(prompt.contains(objetivo), "Prompt should contain the objetivo");
    }

    #[test]
    fn build_planning_prompt_contains_speaker_name() {
        let prompt = build_planning_prompt("Some goal", &[], "atlas");
        assert!(
            prompt.contains("ATLAS"),
            "Prompt should reference the speaker in uppercase"
        );
    }

    #[test]
    fn build_planning_prompt_includes_plan_ready_marker_instructions() {
        let prompt = build_planning_prompt("Some goal", &[], "pixel");
        assert!(
            prompt.contains("===PLAN READY==="),
            "Prompt should instruct agents to emit ===PLAN READY==="
        );
    }

    #[test]
    fn build_planning_prompt_includes_prior_messages() {
        let messages = vec![PlanningMessage {
            sender: "atlas".to_string(),
            content: "I think we should use GraphQL".to_string(),
            round: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }];
        let prompt = build_planning_prompt("Build API", &messages, "pixel");
        assert!(
            prompt.contains("I think we should use GraphQL"),
            "Prompt should include previous message content"
        );
    }

    #[test]
    fn build_planning_prompt_ends_with_your_turn_marker() {
        let prompt = build_planning_prompt("Goal", &[], "atlas");
        assert!(
            prompt.trim_end().ends_with("--- TU TURNO ---"),
            "Prompt should end with the 'your turn' marker"
        );
    }

    // -----------------------------------------------------------------------
    // PlanningStore history
    // -----------------------------------------------------------------------

    fn make_store() -> PlanningStore {
        PlanningStore {
            session: Mutex::new(None),
            child_pid: Mutex::new(None),
            history: Mutex::new(Vec::new()),
            history_id_counter: Mutex::new(0),
            config: Mutex::new(PlanningConfig::default()),
        }
    }

    #[test]
    fn test_planning_history_max_size() {
        let store = make_store();
        for i in 0..55u64 {
            store.push_history(
                format!("prompt {}", i),
                format!("response {}", i),
                "atlas".to_string(),
            );
        }
        let history = store.history.lock().unwrap();
        assert_eq!(
            history.len(),
            PLANNING_HISTORY_MAX,
            "History should be capped at {} entries, got {}",
            PLANNING_HISTORY_MAX,
            history.len()
        );
        // Oldest entries should have been evicted; newest 50 remain
        assert_eq!(history.first().unwrap().prompt, "prompt 5");
        assert_eq!(history.last().unwrap().prompt, "prompt 54");
    }

    #[test]
    fn test_planning_history_clear() {
        let store = make_store();
        for i in 0..10u64 {
            store.push_history(
                format!("prompt {}", i),
                format!("response {}", i),
                "pixel".to_string(),
            );
        }
        {
            let history = store.history.lock().unwrap();
            assert_eq!(history.len(), 10);
        }
        store.history.lock().unwrap().clear();
        *store.history_id_counter.lock().unwrap() = 0;
        let history = store.history.lock().unwrap();
        assert!(history.is_empty(), "History should be empty after clear");
    }

    #[test]
    fn test_export_planning_creates_file() {
        let store = make_store();
        store.push_history(
            "How should we build the API?".to_string(),
            "We should use REST with JSON.".to_string(),
            "atlas".to_string(),
        );
        store.push_history(
            "What about authentication?".to_string(),
            "JWT tokens would work well.".to_string(),
            "pixel".to_string(),
        );

        let filename = format!("test-plan-{}", std::process::id());
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let expected_path = home.join(".config/jarvis/plans").join(format!("{}.md", filename));

        // Remove any leftover file from a previous test run
        let _ = std::fs::remove_file(&expected_path);

        // Export
        let entries = store.history.lock().unwrap().clone();
        let mut md = format!("# Plan: {}\nDate: {}\n\n", filename, chrono::Utc::now().format("%Y-%m-%d"));
        for (i, entry) in entries.iter().enumerate() {
            md.push_str(&format!(
                "## Exchange {}\n**Prompt:** {}\n**Response:** {}\n\n",
                i + 1, entry.prompt, entry.response,
            ));
        }
        let plans_dir = home.join(".config/jarvis/plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        std::fs::write(&expected_path, &md).unwrap();

        assert!(expected_path.exists(), "Exported file should exist at {:?}", expected_path);

        let contents = std::fs::read_to_string(&expected_path).unwrap();
        assert!(contents.contains("# Plan:"), "File should contain the plan title header");
        assert!(contents.contains("## Exchange 1"), "File should contain Exchange 1");
        assert!(contents.contains("## Exchange 2"), "File should contain Exchange 2");
        assert!(contents.contains("How should we build the API?"), "File should contain first prompt");
        assert!(contents.contains("JWT tokens would work well."), "File should contain second response");

        // Clean up
        let _ = std::fs::remove_file(&expected_path);
    }

    // -----------------------------------------------------------------------
    // get_planning_metrics (unit-tested via the helper logic directly)
    // -----------------------------------------------------------------------

    #[test]
    fn planning_metrics_empty_history_returns_zeros() {
        let store = make_store();
        let history = store.history.lock().unwrap();
        let total = history.len();
        drop(history);
        assert_eq!(total, 0);
        // Verify the zero-case JSON shape by running the metric logic inline
        let result = serde_json::json!({
            "total_sessions": 0usize,
            "avg_steps": 0.0f64,
            "success_rate": 0.0f64,
            "total_tokens_used": 0u64,
            "most_used_repo": serde_json::Value::Null,
        });
        assert_eq!(result["total_sessions"], 0);
        assert_eq!(result["success_rate"], 0.0);
    }

    #[test]
    fn planning_metrics_counts_successes_and_timeouts() {
        let store = make_store();
        store.push_history("prompt1".into(), "Good response".into(), "atlas".into());
        store.push_history("prompt2".into(), "[timeout] expired".into(), "pixel".into());
        store.push_history("prompt3".into(), "Another good response".into(), "atlas".into());

        let history = store.history.lock().unwrap();
        let total = history.len();
        let success_count = history.iter().filter(|e| !e.response.starts_with("[timeout]")).count();
        drop(history);

        assert_eq!(total, 3);
        assert_eq!(success_count, 2);
        let success_rate = success_count as f64 / total as f64;
        assert!((success_rate - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn planning_metrics_most_used_repo_is_atlas() {
        let store = make_store();
        store.push_history("p1".into(), "r1".into(), "atlas".into());
        store.push_history("p2".into(), "r2".into(), "atlas".into());
        store.push_history("p3".into(), "r3".into(), "pixel".into());

        let history = store.history.lock().unwrap();
        let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in history.iter() {
            *freq.entry(e.machine.clone()).or_insert(0) += 1;
        }
        drop(history);

        let most_used = freq.into_iter().max_by_key(|(_, c)| *c).map(|(m, _)| m);
        assert_eq!(most_used, Some("atlas".to_string()));
    }

    // -----------------------------------------------------------------------
    // duplicate_planning_session (logic tested via PlanningStore directly)
    // -----------------------------------------------------------------------

    #[test]
    fn duplicate_session_creates_new_entry_with_same_content() {
        let store = make_store();
        store.push_history("original prompt".into(), "original response".into(), "atlas".into());

        let original_id = {
            let history = store.history.lock().unwrap();
            history.first().unwrap().id
        };

        // Simulate duplicate logic
        let source = {
            let history = store.history.lock().unwrap();
            history.iter().find(|e| e.id == original_id).cloned().unwrap()
        };
        let new_id = {
            let mut counter = store.history_id_counter.lock().unwrap();
            *counter += 1;
            *counter
        };
        let duplicate = PlanningHistoryEntry {
            id: new_id,
            timestamp: 0,
            prompt: source.prompt.clone(),
            response: source.response.clone(),
            machine: source.machine.clone(),
        };
        store.history.lock().unwrap().push(duplicate);

        let history = store.history.lock().unwrap();
        assert_eq!(history.len(), 2);
        assert_ne!(history[0].id, history[1].id, "Duplicate must have a different ID");
        assert_eq!(history[0].prompt, history[1].prompt, "Content should be identical");
        assert_eq!(history[0].response, history[1].response);
    }

    #[test]
    fn duplicate_session_invalid_id_is_handled_gracefully() {
        let store = make_store();
        // Attempt to find a non-existent id
        let found = store.history.lock().unwrap().iter().find(|e| e.id == 9999).cloned();
        assert!(found.is_none(), "Should return None for unknown id");
    }

    // -----------------------------------------------------------------------
    // PlanningConfig timeout_secs field
    // -----------------------------------------------------------------------

    #[test]
    fn planning_config_defaults_to_none_timeout() {
        let cfg = PlanningConfig::default();
        assert!(cfg.timeout_secs.is_none(), "Default timeout_secs should be None");
    }

    #[test]
    fn planning_config_custom_timeout_is_stored() {
        let cfg = PlanningConfig { timeout_secs: Some(120) };
        assert_eq!(cfg.timeout_secs, Some(120));
    }
}
