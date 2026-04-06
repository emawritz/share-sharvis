use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Shell-escape a string by wrapping in single quotes and escaping embedded quotes.
/// Prevents command injection when interpolating user values into shell command strings.
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Structured error enum for JARVIS backend operations.
/// All variants carry a descriptive message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum JarvisError {
    /// SSH connectivity or execution failure
    Ssh(String),
    /// I/O error (file system, pipes, etc.)
    Io(String),
    /// Configuration problem (missing key, invalid value, etc.)
    Config(String),
    /// Resource or entity not found
    NotFound(String),
    /// Command blocked by safety rules or capability checks
    CommandBlocked(String),
}

impl fmt::Display for JarvisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JarvisError::Ssh(msg) => write!(f, "SSH error: {}", msg),
            JarvisError::Io(msg) => write!(f, "IO error: {}", msg),
            JarvisError::Config(msg) => write!(f, "Config error: {}", msg),
            JarvisError::NotFound(msg) => write!(f, "Not found: {}", msg),
            JarvisError::CommandBlocked(msg) => write!(f, "Command blocked: {}", msg),
        }
    }
}

/// Tauri commands return `Result<T, String>`, so we convert JarvisError → String.
impl From<JarvisError> for String {
    fn from(e: JarvisError) -> Self {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// App version / environment types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

/// Returns the application version from the Cargo.toml [package] version string.
#[tauri::command]
pub fn get_app_version() -> AppVersion {
    let version_str = env!("CARGO_PKG_VERSION"); // e.g. "0.1.0"
    let parts: Vec<u8> = version_str
        .splitn(3, '.')
        .map(|p| p.parse::<u8>().unwrap_or(0))
        .collect();
    AppVersion {
        major: parts.first().copied().unwrap_or(0),
        minor: parts.get(1).copied().unwrap_or(0),
        patch: parts.get(2).copied().unwrap_or(0),
    }
}

/// Returns environment metadata useful for diagnostics / about screens.
/// All values are gathered at compile-time or from standard library calls — no
/// extra Cargo dependencies required.
#[tauri::command]
pub fn get_environment_info() -> serde_json::Value {
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    };

    let hostname = std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let os_version = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    serde_json::json!({
        "platform": platform,
        "arch": arch,
        "hostname": hostname,
        "osVersion": os_version,
        "cargoVersion": env!("CARGO_PKG_VERSION"),
        "rustEdition": "2021",
    })
}

// ---------------------------------------------------------------------------
// Machine types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub host: String,
    pub ip: Option<String>,
    pub os: String,
    pub role: String,
    pub repo: Option<String>,
    pub repo_path: Option<String>,
    pub gpu: Option<String>,
    pub enabled: bool,
    pub tags: Vec<String>,
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    /// Remote home directory (e.g. "/home/pixel"). Used to expand ~ in remote repo paths.
    /// Defaults to "/home/<machine-id>" if not set.
    #[serde(default)]
    pub home_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoConfig {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub github: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCheck {
    pub name: String,
    pub status: String,       // "ok", "error", "warning"
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineConnections {
    pub machine_id: String,
    pub checks: Vec<ConnectionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineHealth {
    pub online: bool,
    pub latency_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineStats {
    pub cpu: String,
    pub mem: String,
    pub disk: String,
    pub gpu: Option<String>,
    pub uptime: String,
    pub online: bool,
    /// Number of running processes (from `ps aux | wc -l`)
    #[serde(default)]
    pub process_count: Option<u32>,
    /// First non-loopback IP address of the machine
    #[serde(default)]
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineInfo {
    #[serde(flatten)]
    pub machine: Machine,
    pub health: Option<MachineHealth>,
    pub stats: Option<MachineStats>,
}

// ---------------------------------------------------------------------------
// Task types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: u64,
    pub target: String,
    pub prompt: String,
    pub status: String,
    pub orchestrate: bool,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub output: String,
    pub pixel_task_id: Option<u64>,
    #[serde(default)]
    pub depends_on: Vec<u64>,
    #[serde(default = "default_run_condition")]
    pub run_condition: String,
}

fn default_run_condition() -> String {
    "on_success".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_plain_string() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[test]
    fn shell_escape_string_with_spaces() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[test]
    fn shell_escape_single_quote() {
        // it's  →  'it'\''s'
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_escape_empty_string() {
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn shell_escape_backslash() {
        // Backslashes are not special inside single-quoted strings; pass through unchanged
        assert_eq!(shell_escape("a\\b"), "'a\\b'");
    }

    #[test]
    fn shell_escape_multiple_single_quotes() {
        // can't  →  'can'\\''t'
        // won't  →  'won'\\''t'
        assert_eq!(shell_escape("can't won't"), "'can'\\''t won'\\''t'");
    }

    // -- JarvisError tests --

    #[test]
    fn jarvis_error_display_ssh() {
        let e = JarvisError::Ssh("connection refused".to_string());
        assert_eq!(e.to_string(), "SSH error: connection refused");
    }

    #[test]
    fn jarvis_error_display_not_found() {
        let e = JarvisError::NotFound("machine atlas".to_string());
        assert_eq!(e.to_string(), "Not found: machine atlas");
    }

    #[test]
    fn jarvis_error_into_string() {
        let e = JarvisError::Config("missing key: host".to_string());
        let s: String = e.into();
        assert_eq!(s, "Config error: missing key: host");
    }

    #[test]
    fn jarvis_error_command_blocked() {
        let e = JarvisError::CommandBlocked("rm -rf /".to_string());
        assert!(e.to_string().starts_with("Command blocked:"));
    }

    // -- AppVersion tests --

    #[test]
    fn get_app_version_returns_valid_semver() {
        let v = get_app_version();
        // version string is taken from Cargo.toml; it must parse without overflow
        // Just verify the struct is populated (major.minor.patch all fit in u8)
        let _ = format!("{}.{}.{}", v.major, v.minor, v.patch);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskChainStep {
    pub target: String,
    pub prompt: String,
    #[serde(default = "default_run_condition")]
    pub run_condition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGraphNode {
    pub id: String,            // local reference ID (e.g. "lint", "test", "build")
    pub target: String,        // machine target
    pub prompt: String,
    #[serde(default)]
    pub depends_on: Vec<String>, // IDs of nodes that must complete first
    #[serde(default = "default_on_failure")]
    pub on_failure: String,    // "stop" | "continue" | "skip_dependents"
}

fn default_on_failure() -> String {
    "stop".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGraph {
    pub nodes: Vec<TaskGraphNode>,
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStepDefinition {
    pub name: String,
    pub target: String,
    pub prompt: String,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default = "default_on_fail")]
    pub on_fail: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Optional action type: "task" (default), "open_pr", "merge_when_green"
    #[serde(default)]
    pub action: Option<String>,
}

fn default_on_fail() -> String {
    "stop".to_string()
}

fn default_max_retries() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineDefinition {
    pub name: String,
    pub description: String,
    pub steps: Vec<PipelineStepDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStepState {
    pub name: String,
    pub target: String,
    pub status: String,
    pub output: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineState {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub current_step: i32,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub steps: Vec<PipelineStepState>,
}

// ---------------------------------------------------------------------------
// Config types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub rama: String,
    #[serde(default)]
    pub objetivo: String,
}

// ---------------------------------------------------------------------------
// Session types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundInfo {
    pub file: String,
    pub size: u64,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSummary {
    pub file: String,
    pub summary: String,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub active: bool,
    pub session_id: String,
    pub objetivo: String,
    pub rama: String,
    pub total_rounds: String,
    pub atlas_running: bool,
    pub pixel_running: bool,
    pub rounds: Vec<RoundInfo>,
    pub round_summaries: Vec<RoundSummary>,
    pub commits_back: Vec<String>,
    pub commits_front: Vec<String>,
}

// ---------------------------------------------------------------------------
// Agent info types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub agent_count: usize,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetail {
    pub session_id: String,
    pub last_tool: Option<String>,
    pub last_detail: Option<String>,
    pub last_text: Option<String>,
    pub seconds_ago: u64,
}

// ---------------------------------------------------------------------------
// Activity types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ---------------------------------------------------------------------------
// Visibility / Timeline types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub timestamp: String,
    pub role: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub tool_name: Option<String>,
    pub detail: String,
    pub command: Option<String>,
    pub file_path: Option<String>,
    pub tool_use_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub tool_calls: HashMap<String, u64>,
    pub duration: u64,
    pub duration_human: String,
    pub error_count: u64,
    pub files_touched: Vec<String>,
    pub commands_run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorContext {
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub tool: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineError {
    pub timestamp: String,
    pub tool: String,
    pub command: String,
    pub error: String,
    pub context_before: Option<ErrorContext>,
    pub context_after: Option<ErrorContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapEntry {
    pub minute: String,
    pub count: u64,
    pub tools: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub reads: u64,
    pub edits: u64,
    pub writes: u64,
    pub total: u64,
}

// ---------------------------------------------------------------------------
// Token / Cost stats types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStats {
    pub total_cost_usd: f64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub sessions_today: u32,
    pub cost_by_model: HashMap<String, f64>,
}

// ---------------------------------------------------------------------------
// Daily stats / tool stats types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStat {
    pub date: String,
    pub tokens: u64,
    pub cost_usd: f64,
    pub events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub tool_name: String,
    pub calls: u64,
}

// ---------------------------------------------------------------------------
// GitHub types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PR {
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub head_ref_name: String,
    #[serde(default)]
    pub author: serde_json::Value,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub reviews: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub changes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareCommit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchComparison {
    pub ahead_by: u64,
    pub behind_by: u64,
    pub total_commits: u64,
    pub files: Vec<CompareFile>,
    pub commits: Vec<CompareCommit>,
}

// ---------------------------------------------------------------------------
// Timeline response (combined)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResponse {
    pub summary: TimelineSummary,
    pub errors: Vec<TimelineError>,
    pub heatmap: Vec<HeatmapEntry>,
    pub files: Vec<FileChange>,
    pub event_count: usize,
}

// ---------------------------------------------------------------------------
// Agent log types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub type_: String, // "tool_use", "tool_result", "text", "prompt"
    pub tool_name: Option<String>,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub is_error: bool,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedState {
    pub tasks: Vec<Task>,
    pub task_id_counter: u64,
    pub conversation_history: HashMap<String, Vec<ConversationEntry>>,
    pub config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEntry {
    pub id: u64,
    pub prompt: String,
    pub output: String,
}

// ---------------------------------------------------------------------------
// Planning history types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningHistoryEntry {
    pub id: u64,
    pub timestamp: u64,
    pub prompt: String,
    pub response: String,
    pub machine: String,
}

// ---------------------------------------------------------------------------
// Planning types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub branch: String,
    pub changed: u32,
    pub staged: u32,
    pub untracked: u32,
    pub last_commit: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningMessage {
    pub sender: String,
    pub content: String,
    pub round: u32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub index: usize,
    pub target: String,
    pub description: String,
    pub status: String,
    pub task_id: Option<u64>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningState {
    pub id: String,
    pub objetivo: String,
    pub phase: String,
    pub messages: Vec<PlanningMessage>,
    pub plan_steps: Vec<PlanStep>,
    pub current_round: u32,
    pub current_speaker: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub elapsed_secs: u64,
    pub current_activity: Vec<Activity>,
    pub branch_back: Option<String>,
    pub branch_front: Option<String>,
    pub repo_back: Option<RepoStatus>,
    pub repo_front: Option<RepoStatus>,
    #[serde(default)]
    pub streaming_text: Option<String>,
}
