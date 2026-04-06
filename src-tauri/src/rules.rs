use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCondition {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleAction {
    pub action_type: String,
    pub target: Option<String>,
    pub prompt: Option<String>,
    pub pipeline_name: Option<String>,
    pub message: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRule {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub condition: Option<RuleCondition>,
    pub action: RuleAction,
    pub enabled: bool,
    pub last_fired: Option<String>,
    pub fire_count: u64,
    /// Lower values run first. Defaults to 0.
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFireEvent {
    pub rule_id: String,
    pub timestamp: String,
    pub trigger: String,
    pub result: String,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn evaluate_condition(condition: &Option<RuleCondition>, context: &RuleContext) -> bool {
    let cond = match condition {
        Some(c) => c,
        None => return true,
    };
    let actual = match cond.field.as_str() {
        "target" => &context.target,
        "output" => &context.output,
        _ => return true,
    };
    match cond.operator.as_str() {
        "is" => actual == &cond.value,
        "contains" => actual.contains(&cond.value),
        "not_contains" => !actual.contains(&cond.value),
        _ => true,
    }
}

pub struct RuleContext {
    pub target: String,
    pub output: String,
}

/// Evaluate rules against a trigger. Uses the shared `Db` state via `AppHandle`
/// to avoid opening a separate connection that would race with Tauri command handlers.
pub fn evaluate_rules(trigger: &str, context: &RuleContext, app: &tauri::AppHandle) -> Vec<(String, RuleAction)> {
    use crate::db;

    let db = app.state::<db::Db>();

    // Load rules with a brief lock, then release immediately
    let mut rules: Vec<AutoRule> = {
        let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
        db::db_load_rules(&conn)
    };

    // Evaluate entirely without holding any lock
    let mut actions = Vec::new();
    let mut fired_rules: Vec<(AutoRule, RuleFireEvent)> = Vec::new();

    for rule in rules.iter_mut() {
        if !rule.enabled || rule.trigger != trigger {
            continue;
        }
        if !evaluate_condition(&rule.condition, context) {
            continue;
        }

        rule.last_fired = Some(now_iso());
        rule.fire_count += 1;
        actions.push((rule.name.clone(), rule.action.clone()));

        let event = RuleFireEvent {
            rule_id: rule.id.clone(),
            timestamp: now_iso(),
            trigger: trigger.to_string(),
            result: format!("Fired: {}", rule.action.action_type),
        };
        fired_rules.push((rule.clone(), event));
    }

    // Persist each fired rule with brief per-write locks
    for (rule, event) in &fired_rules {
        let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
        db::db_save_rule(&conn, rule);
        db::db_save_rule_history(&conn, event);
    }

    actions
}

pub fn execute_rule_actions(app: &tauri::AppHandle, actions: Vec<(String, RuleAction)>) {
    for (rule_name, action) in actions {
        match action.action_type.as_str() {
            "run_task" => {
                let target = action.target.unwrap_or_else(|| "atlas".to_string());
                let prompt = action.prompt.unwrap_or_default();
                if !prompt.is_empty() {
                    let store = app.state::<crate::tasks::TaskStore>();
                    crate::tasks::send_task_internal(app, &store, &target, &prompt, false);
                }
            }
            "send_webhook" => {
                let message = action.message.unwrap_or_else(|| format!("Regla disparada: {}", rule_name));
                crate::webhooks::send_notification(&rule_name, &message, "yellow", "rule_fired");
            }
            "send_message" => {
                let to = action.to.unwrap_or_else(|| "all".to_string());
                let content = action.message.unwrap_or_default();
                if !content.is_empty() {
                    let _ = crate::messages::send_agent_message(
                        "jarvis".into(), to, "info".into(), content, vec!["auto-rule".into()],
                    );
                }
            }
            "alert" => {
                let message = action.message.unwrap_or_else(|| format!("Regla: {}", rule_name));
                let _ = app.emit("rule-alert", serde_json::json!({
                    "rule": rule_name,
                    "message": message,
                }));
                crate::notifications::send_native(app, "JARVIS - Regla", &message);
            }
            "run_pipeline" => {
                let pipeline_name = action.pipeline_name.unwrap_or_else(|| "unknown".to_string());
                match crate::pipelines::start_pipeline_internal(app, &pipeline_name) {
                    Ok(id) => {
                        log::info!("Rule '{}': started pipeline '{}' (id={})", rule_name, pipeline_name, id);
                    }
                    Err(e) => {
                        log::error!("Rule '{}': failed to start pipeline '{}': {}", rule_name, pipeline_name, e);
                        let _ = app.emit("rule-alert", serde_json::json!({
                            "rule": rule_name,
                            "message": format!("Error al iniciar pipeline '{}': {}", pipeline_name, e),
                        }));
                    }
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn ctx(target: &str, output: &str) -> RuleContext {
        RuleContext {
            target: target.to_string(),
            output: output.to_string(),
        }
    }

    fn cond(field: &str, operator: &str, value: &str) -> Option<RuleCondition> {
        Some(RuleCondition {
            field: field.to_string(),
            operator: operator.to_string(),
            value: value.to_string(),
        })
    }

    fn make_rule(id: &str, name: &str, condition: Option<RuleCondition>, enabled: bool, priority: u32) -> AutoRule {
        AutoRule {
            id: id.to_string(),
            name: name.to_string(),
            trigger: "on_task_fail".to_string(),
            condition,
            action: RuleAction {
                action_type: "alert".to_string(),
                target: None,
                prompt: None,
                pipeline_name: None,
                message: Some("test".to_string()),
                to: None,
            },
            enabled,
            last_fired: None,
            fire_count: 0,
            priority,
        }
    }

    fn open_test_db() -> crate::db::Db {
        use rusqlite::Connection;
        use std::sync::Mutex;

        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                trigger TEXT NOT NULL,
                condition TEXT,
                action TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                fire_count INTEGER NOT NULL DEFAULT 0,
                last_fired TEXT,
                priority INTEGER NOT NULL DEFAULT 0
            );"
        ).expect("create rules table");
        crate::db::Db { conn: Mutex::new(conn) }
    }

    // -----------------------------------------------------------------------
    // evaluate_condition tests (pure logic, no DB)
    // -----------------------------------------------------------------------

    #[test]
    fn evaluate_none_condition_is_always_true() {
        assert!(evaluate_condition(&None, &ctx("atlas", "some output")));
    }

    #[test]
    fn evaluate_is_operator_matching_target() {
        assert!(evaluate_condition(&cond("target", "is", "atlas"), &ctx("atlas", "out")));
    }

    #[test]
    fn evaluate_is_operator_nonmatching_target() {
        assert!(!evaluate_condition(&cond("target", "is", "atlas"), &ctx("pixel", "out")));
    }

    #[test]
    fn evaluate_contains_operator_present() {
        assert!(evaluate_condition(&cond("output", "contains", "test"), &ctx("", "3 tests failed")));
    }

    #[test]
    fn evaluate_contains_operator_absent() {
        assert!(!evaluate_condition(&cond("output", "contains", "test"), &ctx("", "all passed")));
    }

    #[test]
    fn evaluate_not_contains_operator_absent() {
        assert!(evaluate_condition(&cond("output", "not_contains", "error"), &ctx("", "all good")));
    }

    #[test]
    fn evaluate_not_contains_operator_present() {
        assert!(!evaluate_condition(&cond("output", "not_contains", "error"), &ctx("", "error: fail")));
    }

    #[test]
    fn evaluate_unknown_operator_is_true() {
        assert!(evaluate_condition(&cond("target", "regex_match", "atlas"), &ctx("atlas", "")));
    }

    #[test]
    fn evaluate_unknown_field_is_true() {
        assert!(evaluate_condition(&cond("machine_type", "is", "gpu"), &ctx("", "")));
    }

    // -----------------------------------------------------------------------
    // DB-backed tests using in-memory SQLite
    // -----------------------------------------------------------------------

    /// get_rules returns all saved rules
    #[test]
    fn get_rules_returns_all() {
        let db = open_test_db();
        let r1 = make_rule("id-1", "Rule One", None, true, 0);
        let r2 = make_rule("id-2", "Rule Two", None, true, 1);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &r1);
            crate::db::db_save_rule(&conn, &r2);
        }
        let rules = crate::db::db_load_rules(&db.conn.lock().unwrap());
        assert_eq!(rules.len(), 2);
    }

    /// save_rule persists and can be re-loaded with all fields intact
    #[test]
    fn save_rule_persists() {
        let db = open_test_db();
        let rule = make_rule("save-1", "Persisted Rule", cond("output", "contains", "error"), true, 5);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &rule);
        }
        let rules = crate::db::db_load_rules(&db.conn.lock().unwrap());
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "Persisted Rule");
        assert_eq!(rules[0].priority, 5);
        assert!(rules[0].condition.is_some());
    }

    /// toggle_rule: flipping enabled=false persists and reloads correctly
    #[test]
    fn toggle_rule_flips_enabled() {
        let db = open_test_db();
        let rule = make_rule("toggle-1", "Toggle Rule", None, true, 0);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &rule);
        }
        // Flip to disabled
        {
            let conn = db.conn.lock().unwrap();
            let mut rules = crate::db::db_load_rules(&conn);
            if let Some(r) = rules.iter_mut().find(|r| r.id == "toggle-1") {
                r.enabled = false;
                crate::db::db_save_rule(&conn, r);
            }
        }
        let rules = crate::db::db_load_rules(&db.conn.lock().unwrap());
        assert!(!rules[0].enabled);
    }

    /// delete_rule removes the rule so it no longer appears in load
    #[test]
    fn delete_rule_removes() {
        let db = open_test_db();
        let rule = make_rule("del-1", "Delete Me", None, true, 0);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &rule);
        }
        {
            let conn = db.conn.lock().unwrap();
            let removed = crate::db::db_delete_rule(&conn, "del-1");
            assert!(removed);
        }
        let rules = crate::db::db_load_rules(&db.conn.lock().unwrap());
        assert!(rules.is_empty());
    }

    /// reorder_rules: priority ordering is reflected in the DB-loaded order
    #[test]
    fn priority_ordering_after_reorder() {
        let db = open_test_db();
        let r1 = make_rule("p-1", "First", None, true, 0);
        let r2 = make_rule("p-2", "Second", None, true, 1);
        let r3 = make_rule("p-3", "Third", None, true, 2);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &r1);
            crate::db::db_save_rule(&conn, &r2);
            crate::db::db_save_rule(&conn, &r3);
        }
        // Reorder: p-3 first, p-1 second, p-2 third
        let new_order = ["p-3", "p-1", "p-2"];
        {
            let conn = db.conn.lock().unwrap();
            let mut rules = crate::db::db_load_rules(&conn);
            for (new_priority, id) in new_order.iter().enumerate() {
                if let Some(rule) = rules.iter_mut().find(|r| r.id == *id) {
                    rule.priority = new_priority as u32;
                    crate::db::db_save_rule(&conn, rule);
                }
            }
        }
        // DB returns rules ORDER BY priority ASC → p-3(0), p-1(1), p-2(2)
        let rules = crate::db::db_load_rules(&db.conn.lock().unwrap());
        assert_eq!(rules[0].id, "p-3");
        assert_eq!(rules[1].id, "p-1");
        assert_eq!(rules[2].id, "p-2");
    }

    /// dry_run_rule: returns true when the test prompt matches the rule's condition
    #[test]
    fn dry_run_rule_match() {
        let db = open_test_db();
        let rule = make_rule("dry-1", "Error Watcher", cond("output", "contains", "ERROR"), true, 0);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &rule);
        }
        let conn = db.conn.lock().unwrap();
        let rules = crate::db::db_load_rules(&conn);
        let found = rules.iter().find(|r| r.id == "dry-1").unwrap();
        let context = RuleContext { target: String::new(), output: "Build ERROR: missing dep".to_string() };
        assert!(evaluate_condition(&found.condition, &context));
    }

    /// dry_run_rule: returns false when the test prompt does not match
    #[test]
    fn dry_run_rule_no_match() {
        let db = open_test_db();
        let rule = make_rule("dry-2", "Error Watcher", cond("output", "contains", "ERROR"), true, 0);
        {
            let conn = db.conn.lock().unwrap();
            crate::db::db_save_rule(&conn, &rule);
        }
        let conn = db.conn.lock().unwrap();
        let rules = crate::db::db_load_rules(&conn);
        let found = rules.iter().find(|r| r.id == "dry-2").unwrap();
        let context = RuleContext { target: String::new(), output: "Build successful".to_string() };
        assert!(!evaluate_condition(&found.condition, &context));
    }
}

fn ensure_defaults(db: &crate::db::Db) {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let rules = crate::db::db_load_rules(&conn);
    if !rules.is_empty() {
        return;
    }
    let defaults = vec![
        AutoRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Auto-fix tests on fail".into(),
            trigger: "on_task_fail".into(),
            condition: Some(RuleCondition {
                field: "output".into(),
                operator: "contains".into(),
                value: "test".into(),
            }),
            action: RuleAction {
                action_type: "run_task".into(),
                target: Some("atlas".into()),
                prompt: Some("Corri los tests y arregla los que fallen".into()),
                pipeline_name: None,
                message: None,
                to: None,
            },
            enabled: false,
            last_fired: None,
            fire_count: 0,
            priority: 0,
        },
        AutoRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Webhook on pipeline done".into(),
            trigger: "on_pipeline_complete".into(),
            condition: None,
            action: RuleAction {
                action_type: "send_webhook".into(),
                target: None,
                prompt: None,
                pipeline_name: None,
                message: Some("Pipeline completado".into()),
                to: None,
            },
            enabled: false,
            last_fired: None,
            fire_count: 0,
            priority: 0,
        },
        AutoRule {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Notificar equipo en push".into(),
            trigger: "on_push".into(),
            condition: None,
            action: RuleAction {
                action_type: "send_message".into(),
                target: None,
                prompt: None,
                pipeline_name: None,
                message: Some("Nuevos commits detectados en el repo".into()),
                to: Some("all".into()),
            },
            enabled: false,
            last_fired: None,
            fire_count: 0,
            priority: 0,
        },
    ];
    for rule in &defaults {
        crate::db::db_save_rule(&conn, rule);
    }
}

// -- Tauri Commands --

#[tauri::command]
pub fn get_rules(db: tauri::State<'_, crate::db::Db>) -> Vec<AutoRule> {
    ensure_defaults(&db);
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_load_rules(&conn)
}

#[tauri::command]
pub fn save_rule(db: tauri::State<'_, crate::db::Db>, rule: AutoRule) -> AutoRule {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_save_rule(&conn, &rule);
    rule
}

#[tauri::command]
pub fn delete_rule(db: tauri::State<'_, crate::db::Db>, id: String) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_delete_rule(&conn, &id)
}

#[tauri::command]
pub fn toggle_rule(db: tauri::State<'_, crate::db::Db>, id: String, enabled: bool) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let mut rules = crate::db::db_load_rules(&conn);
    if let Some(rule) = rules.iter_mut().find(|r| r.id == id) {
        rule.enabled = enabled;
        crate::db::db_save_rule(&conn, rule);
        true
    } else {
        false
    }
}

#[tauri::command]
pub fn get_rule_history(db: tauri::State<'_, crate::db::Db>, id: Option<String>) -> Vec<RuleFireEvent> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    crate::db::db_load_rule_history(&conn, id.as_deref())
}

/// Reorder rules to match the given ID order.
/// Each rule at position `i` in the `ids` list receives `priority = i`.
#[tauri::command]
pub fn reorder_rules(db: tauri::State<'_, crate::db::Db>, ids: Vec<String>) {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let mut rules = crate::db::db_load_rules(&conn);
    for (new_priority, id) in ids.iter().enumerate() {
        if let Some(rule) = rules.iter_mut().find(|r| &r.id == id) {
            rule.priority = new_priority as u32;
            crate::db::db_save_rule(&conn, rule);
        }
    }
}

/// Check whether `test_prompt` would match the condition of the rule identified by `rule_id`.
/// Returns `true` if the rule exists AND its condition matches (treating the prompt as `output`).
/// Does not execute any action.
#[tauri::command]
pub fn dry_run_rule(db: tauri::State<'_, crate::db::Db>, rule_id: String, test_prompt: String) -> bool {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let rules = crate::db::db_load_rules(&conn);
    let Some(rule) = rules.iter().find(|r| r.id == rule_id) else {
        return false;
    };
    let context = RuleContext {
        target: String::new(),
        output: test_prompt,
    };
    evaluate_condition(&rule.condition, &context)
}
