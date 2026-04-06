use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;

use tauri::{AppHandle, Emitter, Manager};

use crate::tasks::{send_task_internal, TaskStore};
use crate::types::{PipelineDefinition, PipelineState, PipelineStepDefinition, PipelineStepState};

// ---------------------------------------------------------------------------
// Built-in pipeline definitions
// ---------------------------------------------------------------------------

fn builtin_pipelines() -> Vec<PipelineDefinition> {
    vec![
        PipelineDefinition {
            name: "test-fix-loop".into(),
            description: "Run tests, fix failures, rerun -- up to 3 cycles, commit on pass".into(),
            steps: vec![
                PipelineStepDefinition {
                    name: "Run tests".into(),
                    target: "atlas".into(),
                    prompt: "Run the project test suite. Report PASS if all tests pass, or FAIL with the failing test output.".into(),
                    condition: None,
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Fix failing tests (attempt 1)".into(),
                    target: "atlas".into(),
                    prompt: "The previous test run output:\n\n{{prev_output}}\n\nFix the failing tests. Apply minimal changes to make them pass.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Rerun tests (attempt 1)".into(),
                    target: "atlas".into(),
                    prompt: "Run the project test suite again after fixes. Report PASS if all tests pass, or FAIL with details.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Fix failing tests (attempt 2)".into(),
                    target: "atlas".into(),
                    prompt: "The previous test run output:\n\n{{prev_output}}\n\nFix the remaining failing tests.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Rerun tests (attempt 2)".into(),
                    target: "atlas".into(),
                    prompt: "Run the project test suite again. Report PASS or FAIL.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Fix failing tests (attempt 3)".into(),
                    target: "atlas".into(),
                    prompt: "The previous test run output:\n\n{{prev_output}}\n\nLast attempt -- fix the remaining failures.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Final test run".into(),
                    target: "atlas".into(),
                    prompt: "Run the test suite one final time. Report PASS or FAIL.".into(),
                    condition: Some("FAIL".into()),
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Commit on success".into(),
                    target: "atlas".into(),
                    prompt: "All tests pass. Stage changed files and commit with message \"fix: tests passing after auto-fix loop\".".into(),
                    condition: Some("!FAIL".into()),
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
            ],
        },
        PipelineDefinition {
            name: "full-check".into(),
            description: "Lint, build, test, commit if all pass".into(),
            steps: vec![
                PipelineStepDefinition {
                    name: "Lint".into(),
                    target: "atlas".into(),
                    prompt: "Run the project linter. Report PASS if clean or FAIL with issues.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Build".into(),
                    target: "atlas".into(),
                    prompt: "Build the project. Report PASS if successful or FAIL with errors.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Test".into(),
                    target: "atlas".into(),
                    prompt: "Run the full test suite. Report PASS if all pass or FAIL with details.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Commit".into(),
                    target: "atlas".into(),
                    prompt: "All checks passed (lint, build, test). Stage and commit with message \"chore: full-check passed\".".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
            ],
        },
        PipelineDefinition {
            name: "deploy-prep".into(),
            description: "Pull, install deps, build, test, create PR".into(),
            steps: vec![
                PipelineStepDefinition {
                    name: "Pull latest".into(),
                    target: "atlas".into(),
                    prompt: "Pull the latest changes from the main branch. Report the result.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Install dependencies".into(),
                    target: "atlas".into(),
                    prompt: "Install project dependencies (npm install or equivalent). Report PASS or FAIL.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Build".into(),
                    target: "atlas".into(),
                    prompt: "Build the project for production. Report PASS or FAIL with any errors.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Test".into(),
                    target: "atlas".into(),
                    prompt: "Run the full test suite. Report PASS or FAIL.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Create PR".into(),
                    target: "atlas".into(),
                    prompt: "Create a pull request from the current branch to main using `gh pr create`. Include a summary of recent commits in the PR body.".into(),
                    condition: None,
                    on_fail: "stop".into(),
                    max_retries: 1,
                    action: None,
                },
            ],
        },
        PipelineDefinition {
            name: "sync-repos".into(),
            description: "Git pull on both machines and show status".into(),
            steps: vec![
                PipelineStepDefinition {
                    name: "Pull on ATLAS".into(),
                    target: "atlas".into(),
                    prompt: "Run `git pull` and then `git status`. Report the full output.".into(),
                    condition: None,
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
                PipelineStepDefinition {
                    name: "Pull on PIXEL".into(),
                    target: "pixel".into(),
                    prompt: "Run `git pull` and then `git status`. Report the full output.".into(),
                    condition: None,
                    on_fail: "next".into(),
                    max_retries: 1,
                    action: None,
                },
            ],
        },
    ]
}

// ---------------------------------------------------------------------------
// Pipeline store
// ---------------------------------------------------------------------------

pub struct PipelineStore {
    pub pipelines: Mutex<HashMap<String, PipelineState>>,
}

impl PipelineStore {
    pub fn new() -> Self {
        Self {
            pipelines: Mutex::new(HashMap::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Condition evaluator
// ---------------------------------------------------------------------------

/// Evaluate a simple (leaf) condition string against `output`.
fn evaluate_simple_condition(cond: &str, output: &str) -> bool {
    if cond == "FAIL" {
        output.contains("FAIL")
    } else if cond == "!FAIL" {
        !output.contains("FAIL")
    } else if let Some(rest) = cond.strip_prefix("contains:") {
        output.contains(rest)
    } else if let Some(rest) = cond.strip_prefix("!contains:") {
        !output.contains(rest)
    } else {
        // Default: check if output contains the condition string
        output.contains(cond)
    }
}

/// Evaluate a condition that may be a compound JSON expression or a simple string.
///
/// Supported JSON forms:
///   `{"and": [...]}` — all children must be true
///   `{"or":  [...]}` — any child must be true
///   `{"not": "..."}` — negate a single child
///
/// Children may themselves be JSON objects (nested) or plain strings (leaf).
fn evaluate_condition_recursive(condition: &str, output: &str) -> bool {
    let trimmed = condition.trim();
    if !trimmed.starts_with('{') {
        // Fast path: plain string
        return evaluate_simple_condition(trimmed, output);
    }

    // Try to parse as JSON
    let val: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            // Not valid JSON — fall back to string comparison
            return evaluate_simple_condition(trimmed, output);
        }
    };

    if let Some(children) = val.get("and").and_then(|v| v.as_array()) {
        return children.iter().all(|child| {
            let s = match child {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            evaluate_condition_recursive(&s, output)
        });
    }

    if let Some(children) = val.get("or").and_then(|v| v.as_array()) {
        return children.iter().any(|child| {
            let s = match child {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            evaluate_condition_recursive(&s, output)
        });
    }

    if let Some(child) = val.get("not") {
        let s = match child {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        return !evaluate_condition_recursive(&s, output);
    }

    // Unknown JSON shape — fall back to always-true so we don't silently skip steps
    true
}

fn evaluate_condition(condition: &Option<String>, output: &str) -> bool {
    match condition {
        None => true,
        Some(cond) => evaluate_condition_recursive(cond, output),
    }
}

// ---------------------------------------------------------------------------
// PR helpers
// ---------------------------------------------------------------------------

/// Parse PR parameters from a prompt template.
///
/// The `prompt` field for `open_pr` steps uses a simple key=value block:
/// ```text
/// repo=owner/my-repo
/// title=My PR title {{prev_output}}
/// body=Description here {{prev_output}}
/// ```
/// Any line not matching `key=value` is appended to the body.
/// `{{prev_output}}` in title or body is substituted with `prev_output`.
fn parse_pr_params(prompt: &str, prev_output: &str) -> (String, String, String) {
    let substituted = prompt.replace("{{prev_output}}", prev_output);
    let mut repo = String::new();
    let mut title = String::new();
    let mut body_parts: Vec<String> = Vec::new();

    for line in substituted.lines() {
        if let Some(v) = line.strip_prefix("repo=") {
            repo = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("title=") {
            title = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("body=") {
            body_parts.push(v.trim().to_string());
        } else {
            body_parts.push(line.to_string());
        }
    }

    if title.is_empty() {
        title = "Automated PR".to_string();
    }

    (repo, title, body_parts.join("\n").trim().to_string())
}

fn resolve_prompt(template: &str, prev_output: &str) -> String {
    template.replace(
        "{{prev_output}}",
        if prev_output.is_empty() {
            "(no previous output)"
        } else {
            prev_output
        },
    )
}

// ---------------------------------------------------------------------------
// Pipeline runner
// ---------------------------------------------------------------------------

fn run_pipeline_blocking(
    app: &AppHandle,
    pipeline_id: &str,
    definition: &PipelineDefinition,
    pipeline_store: &PipelineStore,
    task_store: &TaskStore,
) {
    let now_iso = || chrono::Utc::now().to_rfc3339();

    // Mark as running
    {
        let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = pipelines.get_mut(pipeline_id) {
            state.status = "running".into();
            state.started_at = Some(now_iso());
        }
    }

    let mut prev_output = String::new();

    for (i, step_def) in definition.steps.iter().enumerate() {
        // Check if pipeline was cancelled externally
        {
            let pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = pipelines.get(&pipeline_id.to_string()) {
                if state.status == "cancelling" {
                    drop(pipelines);
                    let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(state) = pipelines.get_mut(pipeline_id) {
                        for j in i..state.steps.len() {
                            if state.steps[j].status == "pending" || state.steps[j].status == "running" {
                                state.steps[j].status = "cancelled".into();
                            }
                        }
                        state.status = "cancelled".into();
                        state.finished_at = Some(now_iso());
                    }
                    return;
                }
            }
        }

        // Update current step
        {
            let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = pipelines.get_mut(pipeline_id) {
                state.current_step = i as i32;
            }
        }

        // Condition check
        if !evaluate_condition(&step_def.condition, &prev_output) {
            let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = pipelines.get_mut(pipeline_id) {
                if let Some(step) = state.steps.get_mut(i) {
                    step.status = "skipped".into();
                    let now = now_iso();
                    step.started_at = Some(now.clone());
                    step.finished_at = Some(now);
                }
            }
            let _ = app.emit(
                "pipeline-step",
                serde_json::json!({
                    "pipeline_id": pipeline_id,
                    "step": i,
                    "status": "skipped"
                }),
            );
            continue;
        }

        let max_retries = step_def.max_retries.max(1);
        let mut succeeded = false;

        for attempt in 0..max_retries {
            // Mark step running
            {
                let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(state) = pipelines.get_mut(pipeline_id) {
                    if let Some(step) = state.steps.get_mut(i) {
                        step.status = "running".into();
                        step.retries = attempt;
                        step.started_at = Some(now_iso());
                    }
                }
            }

            let prompt = resolve_prompt(&step_def.prompt, &prev_output);

            let mut combined_output = String::new();
            let mut all_ok = true;

            // Dispatch based on action type
            let action = step_def.action.as_deref().unwrap_or("task");

            match action {
                "open_pr" => {
                    let (repo, title, body) = parse_pr_params(&step_def.prompt, &prev_output);
                    match crate::github::create_pr_for_pipeline(&repo, &title, &body) {
                        Ok(url) => {
                            combined_output = url;
                            all_ok = true;
                        }
                        Err(e) => {
                            combined_output = format!("PR failed: {}", e);
                            all_ok = false;
                        }
                    }
                }
                "merge_when_green" => {
                    let repo = step_def.target.clone();
                    let pr_number: u64 = prev_output.trim()
                        .lines()
                        .next_back()
                        .unwrap_or("")
                        .trim()
                        .split('/')
                        .next_back()
                        .unwrap_or("")
                        .parse()
                        .unwrap_or(0);

                    let shutdown = app.state::<crate::ShutdownFlag>().inner().clone();
                    let mut merged = false;
                    // Poll up to 10 times × 30 s = 5 minutes
                    'poll: for _ in 0..10 {
                        match crate::github::get_checks_for_pipeline(&repo) {
                            Ok(checks) if !checks.is_empty()
                                && checks.iter().all(|c| {
                                    c.status == "completed"
                                        && c.conclusion == "success"
                                }) =>
                            {
                                if let Ok(true) = crate::github::merge_pr_for_pipeline(
                                    &repo, pr_number, "squash",
                                ) {
                                    merged = true;
                                    break 'poll;
                                }
                            }
                            _ => {}
                        }
                        // Break 30s sleep into 1s slices so we can honour shutdown quickly
                        for _ in 0..30 {
                            thread::sleep(std::time::Duration::from_secs(1));
                            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                                break 'poll;
                            }
                        }
                    }
                    combined_output = if merged {
                        "Merged successfully".to_string()
                    } else {
                        "Merge timed out or failed".to_string()
                    };
                    all_ok = merged;
                }
                _ => {
                    // Default: send task to agent(s) and poll for completion
                    let targets: Vec<&str> = if step_def.target == "both" {
                        vec!["atlas", "pixel"]
                    } else {
                        vec![&step_def.target]
                    };

                    for target in &targets {
                        let task = send_task_internal(app, task_store, target, &prompt, false);

                        // Wait for task completion (poll) with 30-min timeout
                        let mut poll_ticks: u64 = 0;
                        const MAX_POLL_TICKS: u64 = 900; // 900 × 2s = 30 min
                        let mut timed_out = false;
                        loop {
                            thread::sleep(std::time::Duration::from_secs(2));
                            poll_ticks += 1;

                            // Check for external cancellation
                            {
                                let pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(state) = pipelines.get(&pipeline_id.to_string()) {
                                    if state.status == "cancelling" {
                                        all_ok = false;
                                        combined_output = "[Cancelled by user]".to_string();
                                        break;
                                    }
                                }
                            }

                            if poll_ticks >= MAX_POLL_TICKS {
                                all_ok = false;
                                timed_out = true;
                                if !combined_output.is_empty() {
                                    combined_output.push_str("\n---\n");
                                }
                                combined_output.push_str(&format!(
                                    "[Timeout: step '{}' on '{}' exceeded 30 minutes]",
                                    step_def.name, target
                                ));
                                break;
                            }

                            let tasks = task_store.tasks.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(t) = tasks.iter().find(|t| t.id == task.id) {
                                if t.status == "done" {
                                    if !combined_output.is_empty() {
                                        combined_output.push_str("\n---\n");
                                    }
                                    combined_output.push_str(&t.output);
                                    break;
                                }
                                if t.status == "error" || t.status == "timeout" || t.status == "killed" {
                                    all_ok = false;
                                    if !combined_output.is_empty() {
                                        combined_output.push_str("\n---\n");
                                    }
                                    combined_output.push_str(&t.output);
                                    break;
                                }
                            }
                        }
                        let _ = timed_out; // acknowledged above
                    }
                }
            }

            // Update step state
            {
                let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(state) = pipelines.get_mut(pipeline_id) {
                    if let Some(step) = state.steps.get_mut(i) {
                        step.output = Some(combined_output.clone());
                        step.finished_at = Some(now_iso());
                        if all_ok {
                            step.status = "completed".into();
                            succeeded = true;
                        } else {
                            step.status = "failed".into();
                        }
                    }
                }
            }

            prev_output = combined_output;
            if succeeded {
                break;
            }
        }

        let _ = app.emit(
            "pipeline-step",
            serde_json::json!({
                "pipeline_id": pipeline_id,
                "step": i,
                "status": if succeeded { "completed" } else { "failed" }
            }),
        );

        if !succeeded && step_def.on_fail == "stop" {
            let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = pipelines.get_mut(pipeline_id) {
                // Mark all remaining steps as "cancelled" so the frontend can distinguish
                // them from genuinely pending steps that haven't run yet.
                for j in (i + 1)..state.steps.len() {
                    if state.steps[j].status == "pending" {
                        state.steps[j].status = "cancelled".into();
                    }
                }
                state.status = "stopped".into();
                state.finished_at = Some(now_iso());
            }
            return;
        }
    }

    // Final status
    {
        let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = pipelines.get_mut(pipeline_id) {
            let has_failed = state.steps.iter().any(|s| s.status == "failed");
            state.status = if has_failed {
                "failed".into()
            } else {
                "completed".into()
            };
            state.finished_at = Some(now_iso());
        }
    }

    // Evaluate automation rules for pipeline completion
    let rule_context = crate::rules::RuleContext {
        target: "pipeline".to_string(),
        output: pipeline_id.to_string(),
    };
    let rule_actions = crate::rules::evaluate_rules("on_pipeline_complete", &rule_context, app);
    if !rule_actions.is_empty() {
        crate::rules::execute_rule_actions(app, rule_actions);
    }

    // Native notification
    crate::notifications::send_native(app, "JARVIS - Pipeline", &format!("{} completado", pipeline_id));
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct PipelinesResponse {
    pub pipelines: Vec<PipelineState>,
    pub builtins: Vec<BuiltinInfo>,
}

#[derive(serde::Serialize)]
pub struct BuiltinInfo {
    pub name: String,
    pub description: String,
    pub steps: usize,
}

#[tauri::command]
pub fn get_pipelines(
    pipeline_store: tauri::State<'_, PipelineStore>,
) -> PipelinesResponse {
    let pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
    let builtins = builtin_pipelines();

    PipelinesResponse {
        pipelines: pipelines.values().cloned().collect(),
        builtins: builtins
            .iter()
            .map(|p| BuiltinInfo {
                name: p.name.clone(),
                description: p.description.clone(),
                steps: p.steps.len(),
            })
            .collect(),
    }
}

/// Start a pipeline by name using app state. Returns the pipeline id or an error string.
/// Can be called from Rust code (e.g., rule executor) without going through the Tauri command layer.
pub fn start_pipeline_internal(app: &AppHandle, name: &str) -> Result<String, String> {
    let builtins = builtin_pipelines();
    let def = builtins
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("Pipeline no encontrado: {}", name))?
        .clone();

    let id = uuid::Uuid::new_v4().to_string();

    // Create initial state
    let state = PipelineState {
        id: id.clone(),
        name: def.name.clone(),
        description: def.description.clone(),
        status: "pending".into(),
        current_step: -1,
        started_at: None,
        finished_at: None,
        steps: def
            .steps
            .iter()
            .map(|s| PipelineStepState {
                name: s.name.clone(),
                target: s.target.clone(),
                status: "pending".into(),
                output: None,
                started_at: None,
                finished_at: None,
                retries: 0,
            })
            .collect(),
    };

    let pipeline_store = app.state::<PipelineStore>();
    {
        let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        pipelines.insert(id.clone(), state);
    }

    let pipeline_id = id.clone();
    let app_clone = app.clone();
    thread::spawn(move || {
        let task_store = app_clone.state::<TaskStore>();
        let pipe_store = app_clone.state::<PipelineStore>();
        run_pipeline_blocking(&app_clone, &pipeline_id, &def, &pipe_store, &task_store);
    });

    Ok(id)
}

#[tauri::command]
pub fn run_pipeline(
    app: AppHandle,
    name: String,
) -> Result<String, String> {
    start_pipeline_internal(&app, &name)
}

/// Mark a running pipeline for cancellation. The runner thread will pick this up
/// at the next poll cycle and gracefully terminate.
#[tauri::command]
pub fn stop_pipeline(
    id: String,
    pipeline_store: tauri::State<'_, PipelineStore>,
) -> Result<(), String> {
    let mut pipelines = pipeline_store.pipelines.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = pipelines.get_mut(&id) {
        if state.status == "running" {
            state.status = "cancelling".into();
            Ok(())
        } else {
            Err(format!("Pipeline {} is not running (status: {})", id, state.status))
        }
    } else {
        Err(format!("Pipeline {} not found", id))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // evaluate_condition
    // -----------------------------------------------------------------------

    #[test]
    fn evaluate_condition_none_always_runs() {
        // None condition means "unconditional" — always true regardless of output
        assert!(evaluate_condition(&None, ""));
        assert!(evaluate_condition(&None, "FAIL"));
        assert!(evaluate_condition(&None, "anything"));
    }

    #[test]
    fn evaluate_condition_fail_matches_when_output_contains_fail() {
        assert!(evaluate_condition(&Some("FAIL".into()), "Test result: FAIL"));
    }

    #[test]
    fn evaluate_condition_fail_does_not_match_clean_output() {
        assert!(!evaluate_condition(&Some("FAIL".into()), "All tests passed"));
    }

    #[test]
    fn evaluate_condition_fail_empty_output() {
        assert!(!evaluate_condition(&Some("FAIL".into()), ""));
    }

    #[test]
    fn evaluate_condition_not_fail_matches_when_no_fail_in_output() {
        assert!(evaluate_condition(&Some("!FAIL".into()), "All tests passed"));
    }

    #[test]
    fn evaluate_condition_not_fail_does_not_match_when_fail_present() {
        assert!(!evaluate_condition(&Some("!FAIL".into()), "FAIL: 3 tests failed"));
    }

    #[test]
    fn evaluate_condition_not_fail_with_empty_output() {
        assert!(evaluate_condition(&Some("!FAIL".into()), ""));
    }

    #[test]
    fn evaluate_condition_contains_prefix_matches_substring() {
        assert!(evaluate_condition(
            &Some("contains:BUILD_OK".into()),
            "status: BUILD_OK done"
        ));
    }

    #[test]
    fn evaluate_condition_contains_prefix_no_match() {
        assert!(!evaluate_condition(
            &Some("contains:BUILD_OK".into()),
            "status: BUILD_FAILED"
        ));
    }

    #[test]
    fn evaluate_condition_not_contains_prefix_matches_when_absent() {
        assert!(evaluate_condition(
            &Some("!contains:ERROR".into()),
            "Everything looks fine"
        ));
    }

    #[test]
    fn evaluate_condition_not_contains_prefix_no_match_when_present() {
        assert!(!evaluate_condition(
            &Some("!contains:ERROR".into()),
            "There was an ERROR in the build"
        ));
    }

    #[test]
    fn evaluate_condition_arbitrary_string_uses_contains_fallback() {
        // An unrecognised condition string falls back to: output.contains(cond)
        assert!(evaluate_condition(
            &Some("DEPLOY_READY".into()),
            "system status: DEPLOY_READY"
        ));
    }

    #[test]
    fn evaluate_condition_arbitrary_string_no_match() {
        assert!(!evaluate_condition(
            &Some("DEPLOY_READY".into()),
            "system status: NOT_READY"
        ));
    }

    // -----------------------------------------------------------------------
    // resolve_prompt
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_prompt_substitutes_prev_output() {
        let template = "Previous output was:\n\n{{prev_output}}\n\nFix it.";
        let result = resolve_prompt(template, "Some agent output here.");
        assert_eq!(
            result,
            "Previous output was:\n\nSome agent output here.\n\nFix it."
        );
    }

    #[test]
    fn resolve_prompt_empty_prev_output_uses_placeholder() {
        let template = "Context: {{prev_output}}";
        let result = resolve_prompt(template, "");
        assert_eq!(result, "Context: (no previous output)");
    }

    #[test]
    fn resolve_prompt_whitespace_only_prev_output_is_not_empty() {
        // Only truly empty string triggers the placeholder; whitespace is real content
        let template = "{{prev_output}}";
        let result = resolve_prompt(template, "   ");
        assert_eq!(result, "   ");
    }

    #[test]
    fn resolve_prompt_no_placeholder_returns_template_unchanged() {
        let template = "Run all tests and report PASS or FAIL.";
        let result = resolve_prompt(template, "irrelevant");
        assert_eq!(result, "Run all tests and report PASS or FAIL.");
    }

    #[test]
    fn resolve_prompt_multiple_placeholders_all_substituted() {
        let template = "First: {{prev_output}} | Second: {{prev_output}}";
        let result = resolve_prompt(template, "output");
        assert_eq!(result, "First: output | Second: output");
    }

    #[test]
    fn resolve_prompt_multiline_output_preserved() {
        let template = "Fix this:\n{{prev_output}}\nDone.";
        let prev = "line1\nline2\nline3";
        let result = resolve_prompt(template, prev);
        assert_eq!(result, "Fix this:\nline1\nline2\nline3\nDone.");
    }

    // -----------------------------------------------------------------------
    // builtin_pipelines
    // -----------------------------------------------------------------------

    #[test]
    fn builtin_pipelines_returns_expected_names() {
        let pipelines = builtin_pipelines();
        let names: Vec<&str> = pipelines.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"test-fix-loop"));
        assert!(names.contains(&"full-check"));
        assert!(names.contains(&"deploy-prep"));
        assert!(names.contains(&"sync-repos"));
    }

    #[test]
    fn builtin_pipelines_all_have_at_least_one_step() {
        for pipeline in builtin_pipelines() {
            assert!(
                !pipeline.steps.is_empty(),
                "Pipeline '{}' has no steps",
                pipeline.name
            );
        }
    }

    #[test]
    fn builtin_pipelines_all_steps_have_non_empty_prompt() {
        for pipeline in builtin_pipelines() {
            for (i, step) in pipeline.steps.iter().enumerate() {
                assert!(
                    !step.prompt.trim().is_empty(),
                    "Pipeline '{}' step {} has empty prompt",
                    pipeline.name,
                    i
                );
            }
        }
    }

    #[test]
    fn builtin_pipelines_on_fail_values_are_valid() {
        let valid = ["stop", "next"];
        for pipeline in builtin_pipelines() {
            for (i, step) in pipeline.steps.iter().enumerate() {
                assert!(
                    valid.contains(&step.on_fail.as_str()),
                    "Pipeline '{}' step {} has invalid on_fail: '{}'",
                    pipeline.name,
                    i,
                    step.on_fail
                );
            }
        }
    }

    #[test]
    fn builtin_pipelines_test_fix_loop_has_prev_output_placeholder() {
        let pipelines = builtin_pipelines();
        let loop_pipeline = pipelines.iter().find(|p| p.name == "test-fix-loop").unwrap();
        let has_placeholder = loop_pipeline
            .steps
            .iter()
            .any(|s| s.prompt.contains("{{prev_output}}"));
        assert!(has_placeholder, "test-fix-loop should use {{prev_output}} in at least one step");
    }

    // -----------------------------------------------------------------------
    // evaluate_condition_recursive — compound conditions
    // -----------------------------------------------------------------------

    #[test]
    fn compound_simple_conditions_still_work() {
        // Legacy simple strings must still work through the new recursive path
        assert!(evaluate_condition_recursive("FAIL", "output: FAIL"));
        assert!(!evaluate_condition_recursive("FAIL", "output: ok"));
        assert!(evaluate_condition_recursive("!FAIL", "output: ok"));
        assert!(evaluate_condition_recursive("contains:hello", "say hello world"));
        assert!(!evaluate_condition_recursive("!contains:hello", "say hello world"));
    }

    #[test]
    fn compound_and_both_true() {
        // Both "FAIL" present AND "contains:error" present → true
        let output = "FAIL: error found";
        let cond = r#"{"and": ["FAIL", "contains:error"]}"#;
        assert!(evaluate_condition_recursive(cond, output));
    }

    #[test]
    fn compound_and_one_false() {
        // "FAIL" present but "contains:error" absent → false
        let output = "FAIL: something else";
        let cond = r#"{"and": ["FAIL", "contains:error"]}"#;
        assert!(!evaluate_condition_recursive(cond, output));
    }

    #[test]
    fn compound_or_one_true() {
        // "FAIL" absent but "contains:warning" present → true
        let output = "Build finished with warning";
        let cond = r#"{"or": ["FAIL", "contains:warning"]}"#;
        assert!(evaluate_condition_recursive(cond, output));
    }

    #[test]
    fn compound_or_both_false() {
        let output = "All good";
        let cond = r#"{"or": ["FAIL", "contains:warning"]}"#;
        assert!(!evaluate_condition_recursive(cond, output));
    }

    #[test]
    fn compound_not_negation() {
        // NOT FAIL when output has no FAIL → true
        assert!(evaluate_condition_recursive(r#"{"not": "FAIL"}"#, "All tests passed"));
        // NOT FAIL when output has FAIL → false
        assert!(!evaluate_condition_recursive(r#"{"not": "FAIL"}"#, "FAIL: 1 broken"));
    }

    #[test]
    fn compound_nested_and_or() {
        // {"and": [{"or": ["FAIL", "PASS"]}, "contains:test"]}
        // output contains "PASS" (or true) and "test" (contains true) → true
        let output = "PASS: all tests ok";
        let cond = r#"{"and": [{"or": ["FAIL", "PASS"]}, "contains:test"]}"#;
        assert!(evaluate_condition_recursive(cond, output));
    }

    #[test]
    fn compound_nested_and_or_fails_when_second_false() {
        // output has FAIL but no "test" keyword → false
        let output = "FAIL: build broken";
        let cond = r#"{"and": [{"or": ["FAIL", "PASS"]}, "contains:test"]}"#;
        assert!(!evaluate_condition_recursive(cond, output));
    }

    // -----------------------------------------------------------------------
    // parse_pr_params
    // -----------------------------------------------------------------------

    #[test]
    fn parse_pr_params_basic() {
        let prompt = "repo=owner/repo\ntitle=My PR\nbody=Description here";
        let (repo, title, body) = parse_pr_params(prompt, "");
        assert_eq!(repo, "owner/repo");
        assert_eq!(title, "My PR");
        assert_eq!(body, "Description here");
    }

    #[test]
    fn parse_pr_params_substitutes_prev_output() {
        let prompt = "repo=owner/repo\ntitle=PR for {{prev_output}}\nbody=Details";
        let (repo, title, _body) = parse_pr_params(prompt, "feature-x");
        assert_eq!(repo, "owner/repo");
        assert_eq!(title, "PR for feature-x");
    }

    #[test]
    fn parse_pr_params_default_title_when_missing() {
        let prompt = "repo=owner/repo\nbody=Some body";
        let (_repo, title, _body) = parse_pr_params(prompt, "");
        assert_eq!(title, "Automated PR");
    }

    // -----------------------------------------------------------------------
    // parse_pr_params — additional edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn parse_pr_params_empty_prompt_returns_defaults() {
        let (repo, title, body) = parse_pr_params("", "");
        assert!(repo.is_empty());
        assert_eq!(title, "Automated PR");
        assert!(body.is_empty());
    }

    #[test]
    fn parse_pr_params_body_substitutes_prev_output() {
        let prompt = "repo=org/proj\ntitle=Deploy\nbody=Changes: {{prev_output}}";
        let (_repo, _title, body) = parse_pr_params(prompt, "fix auth bug");
        assert_eq!(body, "Changes: fix auth bug");
    }

    #[test]
    fn parse_pr_params_non_key_lines_appended_to_body() {
        let prompt = "repo=org/proj\ntitle=T\nfree-form line here";
        let (_repo, _title, body) = parse_pr_params(prompt, "");
        assert!(body.contains("free-form line here"));
    }

    #[test]
    fn parse_pr_params_multiline_body_joined_with_newlines() {
        let prompt = "repo=r/r\ntitle=T\nbody=First line\nbody=Second line";
        let (_repo, _title, body) = parse_pr_params(prompt, "");
        assert!(body.contains("First line"));
        assert!(body.contains("Second line"));
    }

    #[test]
    fn parse_pr_params_repo_empty_when_not_provided() {
        let prompt = "title=My PR\nbody=Details";
        let (repo, title, _body) = parse_pr_params(prompt, "");
        assert!(repo.is_empty());
        assert_eq!(title, "My PR");
    }

    // -----------------------------------------------------------------------
    // builtin_pipelines — per-pipeline structural checks
    // -----------------------------------------------------------------------

    #[test]
    fn builtin_pipelines_sync_repos_targets_both_machines() {
        let pipelines = builtin_pipelines();
        let p = pipelines.iter().find(|p| p.name == "sync-repos").unwrap();
        let targets: Vec<&str> = p.steps.iter().map(|s| s.target.as_str()).collect();
        assert!(targets.contains(&"atlas"), "sync-repos should have an atlas step");
        assert!(targets.contains(&"pixel"), "sync-repos should have a pixel step");
    }

    #[test]
    fn builtin_pipelines_deploy_prep_has_pr_step() {
        let pipelines = builtin_pipelines();
        let p = pipelines.iter().find(|p| p.name == "deploy-prep").unwrap();
        let has_pr = p.steps.iter().any(|s| s.prompt.to_lowercase().contains("pull request") || s.name.to_lowercase().contains("pr"));
        assert!(has_pr, "deploy-prep should include a PR creation step");
    }

    #[test]
    fn builtin_pipelines_full_check_has_lint_build_and_test() {
        let pipelines = builtin_pipelines();
        let p = pipelines.iter().find(|p| p.name == "full-check").unwrap();
        let names: Vec<&str> = p.steps.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Lint"), "full-check should have a Lint step");
        assert!(names.contains(&"Build"), "full-check should have a Build step");
        assert!(names.contains(&"Test"), "full-check should have a Test step");
    }

    #[test]
    fn builtin_pipelines_test_fix_loop_last_step_commits() {
        let pipelines = builtin_pipelines();
        let p = pipelines.iter().find(|p| p.name == "test-fix-loop").unwrap();
        let last = p.steps.last().unwrap();
        assert!(
            last.prompt.to_lowercase().contains("commit") || last.name.to_lowercase().contains("commit"),
            "test-fix-loop last step should be a commit step"
        );
    }

    #[test]
    fn builtin_pipelines_max_retries_at_least_one() {
        for pipeline in builtin_pipelines() {
            for (i, step) in pipeline.steps.iter().enumerate() {
                assert!(
                    step.max_retries >= 1,
                    "Pipeline '{}' step {} max_retries must be >= 1, got {}",
                    pipeline.name, i, step.max_retries
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // evaluate_condition — additional edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn evaluate_condition_case_sensitive_fail_lowercase_not_matched() {
        // "FAIL" condition is case-sensitive — lowercase "fail" should not trigger it
        assert!(!evaluate_condition(&Some("FAIL".into()), "test fail: unit test failed"));
    }

    #[test]
    fn evaluate_condition_contains_prefix_is_case_sensitive() {
        // contains: checks are case-sensitive
        assert!(!evaluate_condition(&Some("contains:BUILD_OK".into()), "status: build_ok"));
        assert!(evaluate_condition(&Some("contains:BUILD_OK".into()), "status: BUILD_OK"));
    }

    #[test]
    fn evaluate_condition_compound_and_empty_array_is_vacuously_true() {
        // {"and": []} — all() on empty iterator is vacuously true
        let cond = r#"{"and": []}"#;
        assert!(evaluate_condition_recursive(cond, "any output"));
    }

    #[test]
    fn evaluate_condition_compound_or_empty_array_is_vacuously_false() {
        // {"or": []} — any() on empty iterator is vacuously false
        let cond = r#"{"or": []}"#;
        assert!(!evaluate_condition_recursive(cond, "any output"));
    }

    #[test]
    fn evaluate_condition_invalid_json_falls_back_to_string_comparison() {
        // Starts with '{' but is not valid JSON — treated as a literal string to find in output
        let cond = "{not valid json}";
        assert!(evaluate_condition_recursive(cond, "output: {not valid json} here"));
        assert!(!evaluate_condition_recursive(cond, "output: something else"));
    }
}
