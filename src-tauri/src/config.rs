use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use std::process::Command;

use crate::types::{Machine, RepoConfig};
use crate::machines::MachineRegistry;

const CONFIG_DIR: &str = ".config/jarvis";
const CONFIG_FILE: &str = "config.toml";

/// TOML-serializable config structure
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct JarvisConfig {
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub machines: Vec<MachineConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SessionConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub rama: String,
    #[serde(default)]
    pub objetivo: String,
    #[serde(default)]
    pub native_notifications: Option<bool>,
    #[serde(default)]
    pub keywords_atlas: Option<Vec<String>>,
    #[serde(default)]
    pub keywords_pixel: Option<Vec<String>>,
    #[serde(default)]
    pub budget_limit_usd: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MachineConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default = "default_os")]
    pub os: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub gpu: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub repos: Vec<RepoConfigToml>,
    #[serde(default)]
    pub home_dir: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoConfigToml {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub github: String,
}

fn default_host() -> String { "local".to_string() }
fn default_os() -> String { "linux".to_string() }
fn default_true() -> bool { true }


pub fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(CONFIG_DIR).join(CONFIG_FILE)
}

pub fn config_exists() -> bool {
    config_path().exists()
}

pub fn load_config() -> JarvisConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            log::error!("Failed to parse config {}: {}", path.display(), e);
            JarvisConfig::default()
        }),
        Err(_) => JarvisConfig::default(),
    }
}

pub fn save_config(cfg: &JarvisConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let toml_str = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, toml_str).map_err(|e| e.to_string())?;
    Ok(())
}

/// Convert TOML machines to the app's Machine type
pub fn machines_from_config(cfg: &JarvisConfig) -> HashMap<String, Machine> {
    let mut map = HashMap::new();
    for mc in &cfg.machines {
        let repos: Vec<RepoConfig> = mc.repos.iter().map(|r| RepoConfig {
            name: r.name.clone(),
            path: r.path.clone(),
            github: r.github.clone(),
        }).collect();

        // For backwards compat: first repo becomes repo/repo_path
        let (repo, repo_path) = repos.first()
            .map(|r| (Some(r.name.clone()), Some(r.path.clone())))
            .unwrap_or((None, None));

        map.insert(mc.id.clone(), Machine {
            id: mc.id.clone(),
            name: mc.name.clone(),
            host: mc.host.clone(),
            ip: mc.ip.clone(),
            os: mc.os.clone(),
            role: mc.role.clone(),
            repo,
            repo_path,
            gpu: mc.gpu.clone(),
            enabled: mc.enabled,
            tags: mc.tags.clone(),
            repos,
            home_dir: mc.home_dir.clone(),
        });
    }
    map
}

/// Auto-detect local machine info for first-launch wizard
pub fn detect_local_machine() -> MachineConfig {
    let hostname = std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "local".to_string());

    let os = std::env::consts::OS.to_string();

    let ip = std::process::Command::new("bash")
        .args(["-c", "tailscale ip -4 2>/dev/null | head -1"])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        });

    MachineConfig {
        id: hostname.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect(),
        name: hostname.to_uppercase(),
        host: "local".to_string(),
        ip,
        os,
        role: String::new(),
        gpu: None,
        enabled: true,
        tags: vec!["local".to_string()],
        repos: Vec::new(),
        home_dir: None,
    }
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_jarvis_config() -> JarvisConfig {
    load_config()
}

#[tauri::command]
pub fn save_jarvis_config(
    config: JarvisConfig,
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<bool, String> {
    // Validate machine entries before persisting
    let mut seen_ids = std::collections::HashSet::new();
    for mc in &config.machines {
        if mc.id.is_empty() {
            return Err("machine id must not be empty".into());
        }
        if mc.id.len() > 64 {
            return Err(format!("machine id '{}' too long (max 64 chars)", mc.id));
        }
        if !seen_ids.insert(mc.id.clone()) {
            return Err(format!("duplicate machine id '{}'", mc.id));
        }
        if mc.name.is_empty() {
            return Err(format!("machine '{}' name must not be empty", mc.id));
        }
    }
    save_config(&config)?;
    registry.reload_from_config();
    Ok(true)
}

#[tauri::command]
pub fn is_first_launch() -> bool {
    !config_exists()
}

#[tauri::command]
pub fn get_detected_local() -> MachineConfig {
    detect_local_machine()
}

#[tauri::command]
pub fn test_ssh_connection(host: String) -> crate::types::ConnectionCheck {
    if host.is_empty() {
        return crate::types::ConnectionCheck {
            name: "ssh".to_string(),
            status: "error".to_string(),
            detail: "host must not be empty".to_string(),
        };
    }
    if host.len() > 253 {
        return crate::types::ConnectionCheck {
            name: "ssh".to_string(),
            status: "error".to_string(),
            detail: "host too long".to_string(),
        };
    }
    let start = std::time::Instant::now();
    let output = Command::new("ssh")
        .args(["-o", "ConnectTimeout=5", "-o", "StrictHostKeyChecking=no", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", &host, "echo ok"])
        .output();
    let elapsed = start.elapsed().as_millis();

    match output {
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "ok" => {
            crate::types::ConnectionCheck {
                name: "ssh".to_string(),
                status: "ok".to_string(),
                detail: format!("{}ms", elapsed),
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            crate::types::ConnectionCheck {
                name: "ssh".to_string(),
                status: "error".to_string(),
                detail: if err.is_empty() { "Connection failed".to_string() } else { err },
            }
        }
        Err(e) => crate::types::ConnectionCheck {
            name: "ssh".to_string(),
            status: "error".to_string(),
            detail: e.to_string(),
        },
    }
}

#[tauri::command]
pub fn run_fix_command(machine_id: String, command: String) -> Result<String, String> {
    if command.is_empty() || command.len() > 2000 {
        return Err("Invalid command".into());
    }
    let cfg = load_config();
    let mc = cfg.machines.iter().find(|m| m.id == machine_id)
        .ok_or_else(|| format!("Machine '{}' not found", machine_id))?;

    let output = if mc.host == "local" {
        Command::new("bash")
            .args(["-c", &command])
            .env("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
            .output()
            .map_err(|e| e.to_string())?
    } else {
        Command::new("ssh")
            .args([
                "-o", "ConnectTimeout=15",
                "-o", "StrictHostKeyChecking=no",
                "-o", "ServerAliveInterval=30",
                "-o", "ServerAliveCountMax=5",
                &mc.host,
                &command,
            ])
            .output()
            .map_err(|e| e.to_string())?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(if stdout.is_empty() { "Listo ✓".to_string() } else { stdout })
    } else {
        Err(if stderr.is_empty() { "Command failed".to_string() } else { stderr })
    }
}

#[tauri::command]
pub fn check_machine_connections(machine_id: String) -> crate::types::MachineConnections {
    if machine_id.is_empty() {
        return crate::types::MachineConnections {
            machine_id,
            checks: vec![crate::types::ConnectionCheck {
                name: "machine".into(), status: "error".into(), detail: "machine_id must not be empty".into()
            }],
        };
    }
    let cfg = load_config();
    let mc = cfg.machines.iter().find(|m| m.id == machine_id);

    let Some(mc) = mc else {
        return crate::types::MachineConnections {
            machine_id,
            checks: vec![crate::types::ConnectionCheck {
                name: "machine".into(), status: "error".into(), detail: "Not found".into()
            }],
        };
    };

    let checks = if mc.host == "local" {
        check_local_connections()
    } else {
        check_remote_connections(&mc.host)
    };

    crate::types::MachineConnections { machine_id, checks }
}

fn check_local_connections() -> Vec<crate::types::ConnectionCheck> {
    let mut checks = Vec::new();

    // SSH (local is always ok)
    checks.push(crate::types::ConnectionCheck {
        name: "ssh".into(), status: "ok".into(), detail: "local".into()
    });

    // Tailscale
    let ts = Command::new("bash").args(["-c", "tailscale status --self 2>/dev/null | head -1"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "tailscale".into(),
        status: if ts.is_empty() { "error" } else { "ok" }.into(),
        detail: if ts.is_empty() { "not running".into() } else { ts },
    });

    // Claude CLI
    let claude = Command::new("bash").args(["-c", "claude --version 2>/dev/null"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "claude".into(),
        status: if claude.is_empty() { "error" } else { "ok" }.into(),
        detail: if claude.is_empty() { "not found".into() } else { claude },
    });

    // GitHub CLI
    let gh = Command::new("bash").args(["-c", "gh auth status 2>&1 | head -2"])
        .output().map(|o| {
            let out = String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr);
            out.trim().to_string()
        }).unwrap_or_default();
    let gh_ok = gh.contains("Logged in");
    checks.push(crate::types::ConnectionCheck {
        name: "github".into(),
        status: if gh_ok { "ok" } else { "error" }.into(),
        detail: if gh_ok { "authenticated".into() } else { gh.chars().take(100).collect() },
    });

    // Git
    let git = Command::new("bash").args(["-c", "git --version 2>/dev/null"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "git".into(),
        status: if git.is_empty() { "error" } else { "ok" }.into(),
        detail: if git.is_empty() { "not found".into() } else { git },
    });

    // Node.js
    let node = Command::new("bash").args(["-c", "node --version 2>/dev/null"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "node".into(),
        status: if node.is_empty() { "error" } else { "ok" }.into(),
        detail: if node.is_empty() { "not found".into() } else { node },
    });

    // Python3
    let py = Command::new("bash").args(["-c", "python3 --version 2>&1"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "python".into(),
        status: if py.is_empty() { "error" } else { "ok" }.into(),
        detail: if py.is_empty() { "not found".into() } else { py },
    });

    // Docker
    let docker = Command::new("bash").args(["-c", "docker --version 2>/dev/null | head -1"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "docker".into(),
        status: if docker.is_empty() { "error" } else { "ok" }.into(),
        detail: if docker.is_empty() { "not found".into() } else { docker },
    });

    // Cargo/Rust
    let cargo = Command::new("bash").args(["-c", "cargo --version 2>/dev/null"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    checks.push(crate::types::ConnectionCheck {
        name: "cargo".into(),
        status: if cargo.is_empty() { "error" } else { "ok" }.into(),
        detail: if cargo.is_empty() { "not found".into() } else { cargo },
    });

    // Disk
    let disk = Command::new("bash").args(["-c", "df -h / | tail -1 | awk '{print $4\" free of \"$2}'"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let disk_pct = Command::new("bash").args(["-c", "df -h / | tail -1 | awk '{print $5}'"])
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let disk_num: u32 = disk_pct.trim_end_matches('%').parse().unwrap_or(0);
    checks.push(crate::types::ConnectionCheck {
        name: "disk".into(),
        status: if disk_num > 90 { "warning" } else { "ok" }.into(),
        detail: disk,
    });

    checks
}

/// Parse `===SECTION===`-delimited output into a map of section name → trimmed body.
/// Lines that look like `===NAME===` start a new section; all other lines accumulate
/// into the body of the current section (trimmed).
fn parse_sections(raw: &str) -> std::collections::HashMap<String, String> {
    let mut sections: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current = String::new();
    let mut buf = String::new();
    for line in raw.lines() {
        if line.starts_with("===") && line.ends_with("===") {
            if !current.is_empty() {
                sections.insert(current.clone(), buf.trim().to_string());
            }
            current = line.get(3..line.len().saturating_sub(3)).unwrap_or("").to_string();
            buf.clear();
        } else {
            if !buf.is_empty() { buf.push('\n'); }
            buf.push_str(line);
        }
    }
    if !current.is_empty() {
        sections.insert(current, buf.trim().to_string());
    }
    sections
}

fn check_remote_connections(host: &str) -> Vec<crate::types::ConnectionCheck> {
    let script = concat!(
        "echo '===SSH==='; echo ok;",
        "echo '===TAILSCALE==='; tailscale status --self 2>/dev/null | head -1 || echo '';",
        "echo '===CLAUDE==='; claude --version 2>/dev/null || echo '';",
        "echo '===GH==='; gh auth status 2>&1 | head -2 || echo '';",
        "echo '===GIT==='; git --version 2>/dev/null || echo '';",
        "echo '===NODE==='; node --version 2>/dev/null || echo '';",
        "echo '===PYTHON==='; python3 --version 2>&1 || echo '';",
        "echo '===DOCKER==='; docker --version 2>/dev/null | head -1 || echo '';",
        "echo '===CARGO==='; cargo --version 2>/dev/null || echo '';",
        "echo '===DISK==='; df -h / 2>/dev/null | tail -1 | awk '{print $4\" free of \"$2}' || echo '';",
        "echo '===DISKPCT==='; df -h / 2>/dev/null | tail -1 | awk '{print $5}' || echo '';",
        "echo '===GPU==='; nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || echo '';",
        "echo '===END==='"
    );

    let raw = Command::new("ssh")
        .args(["-o", "ConnectTimeout=5", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", host, script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if raw.trim().is_empty() {
        return vec![crate::types::ConnectionCheck {
            name: "ssh".into(), status: "error".into(), detail: format!("Cannot reach {}", host),
        }];
    }

    // Parse sections
    let sections = parse_sections(&raw);
    let get = |key: &str| sections.get(key).cloned().unwrap_or_default();
    let mut checks = Vec::new();

    checks.push(crate::types::ConnectionCheck {
        name: "ssh".into(), status: "ok".into(), detail: format!("connected to {}", host),
    });

    let ts = get("TAILSCALE");
    checks.push(crate::types::ConnectionCheck {
        name: "tailscale".into(),
        status: if ts.is_empty() { "error" } else { "ok" }.into(),
        detail: if ts.is_empty() { "not running".into() } else { ts },
    });

    let claude = get("CLAUDE");
    checks.push(crate::types::ConnectionCheck {
        name: "claude".into(),
        status: if claude.is_empty() { "error" } else { "ok" }.into(),
        detail: if claude.is_empty() { "not found".into() } else { claude },
    });

    let gh = get("GH");
    let gh_ok = gh.contains("Logged in");
    checks.push(crate::types::ConnectionCheck {
        name: "github".into(),
        status: if gh_ok { "ok" } else { "error" }.into(),
        detail: if gh_ok { "authenticated".into() } else { gh.chars().take(100).collect() },
    });

    let git = get("GIT");
    checks.push(crate::types::ConnectionCheck {
        name: "git".into(),
        status: if git.is_empty() { "error" } else { "ok" }.into(),
        detail: if git.is_empty() { "not found".into() } else { git },
    });

    let node = get("NODE");
    checks.push(crate::types::ConnectionCheck {
        name: "node".into(),
        status: if node.is_empty() { "error" } else { "ok" }.into(),
        detail: if node.is_empty() { "not found".into() } else { node },
    });

    let python = get("PYTHON");
    checks.push(crate::types::ConnectionCheck {
        name: "python".into(),
        status: if python.is_empty() { "error" } else { "ok" }.into(),
        detail: if python.is_empty() { "not found".into() } else { python },
    });

    let docker = get("DOCKER");
    checks.push(crate::types::ConnectionCheck {
        name: "docker".into(),
        status: if docker.is_empty() { "error" } else { "ok" }.into(),
        detail: if docker.is_empty() { "not found".into() } else { docker },
    });

    let cargo = get("CARGO");
    checks.push(crate::types::ConnectionCheck {
        name: "cargo".into(),
        status: if cargo.is_empty() { "error" } else { "ok" }.into(),
        detail: if cargo.is_empty() { "not found".into() } else { cargo },
    });

    let disk = get("DISK");
    let disk_pct_str = get("DISKPCT");
    let disk_num: u32 = disk_pct_str.trim_end_matches('%').parse().unwrap_or(0);
    checks.push(crate::types::ConnectionCheck {
        name: "disk".into(),
        status: if disk_num > 90 { "warning" } else { "ok" }.into(),
        detail: disk,
    });

    let gpu = get("GPU");
    if !gpu.is_empty() {
        checks.push(crate::types::ConnectionCheck {
            name: "gpu".into(),
            status: "ok".into(),
            detail: gpu,
        });
    }

    checks
}

#[tauri::command]
pub fn get_budget_limit() -> Option<f64> {
    load_config().session.budget_limit_usd
}

#[tauri::command]
pub fn set_budget_limit(limit: Option<f64>) -> bool {
    let mut cfg = load_config();
    cfg.session.budget_limit_usd = limit;
    save_config(&cfg).is_ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sections_single_section() {
        let raw = "===SSH===\nok\n";
        let sections = parse_sections(raw);
        assert_eq!(sections.get("SSH").map(|s| s.as_str()), Some("ok"));
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn parse_sections_multiple_sections() {
        let raw = "===SSH===\nok\n===CLAUDE===\n1.2.3\n===GIT===\ngit version 2.39.0";
        let sections = parse_sections(raw);
        assert_eq!(sections.get("SSH").map(|s| s.as_str()), Some("ok"));
        assert_eq!(sections.get("CLAUDE").map(|s| s.as_str()), Some("1.2.3"));
        assert_eq!(sections.get("GIT").map(|s| s.as_str()), Some("git version 2.39.0"));
        assert_eq!(sections.len(), 3);
    }

    #[test]
    fn parse_sections_no_sections_in_input() {
        let raw = "just some plain output\nno markers here";
        let sections = parse_sections(raw);
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_sections_section_with_empty_body() {
        let raw = "===SSH===\n===CLAUDE===\nsome output";
        let sections = parse_sections(raw);
        // SSH section body is empty string (trimmed)
        assert_eq!(sections.get("SSH").map(|s| s.as_str()), Some(""));
        assert_eq!(sections.get("CLAUDE").map(|s| s.as_str()), Some("some output"));
    }

    #[test]
    fn parse_sections_trims_body_whitespace() {
        let raw = "===FOO===\n  \n  padded  \n  \n===END===\n";
        let sections = parse_sections(raw);
        assert_eq!(sections.get("FOO").map(|s| s.as_str()), Some("padded"));
    }

    #[test]
    fn parse_sections_multiline_body() {
        let raw = "===SECTION===\nline1\nline2\nline3";
        let sections = parse_sections(raw);
        assert_eq!(
            sections.get("SECTION").map(|s| s.as_str()),
            Some("line1\nline2\nline3")
        );
    }

    #[test]
    fn parse_sections_end_marker_creates_empty_section() {
        // ===END=== is parsed as a section named "END" with empty body
        let raw = "===SSH===\nok\n===END===\n";
        let sections = parse_sections(raw);
        assert_eq!(sections.get("SSH").map(|s| s.as_str()), Some("ok"));
        assert_eq!(sections.get("END").map(|s| s.as_str()), Some(""));
    }

    // -----------------------------------------------------------------------
    // Default value functions
    // -----------------------------------------------------------------------

    #[test]
    fn default_host_returns_local() {
        assert_eq!(default_host(), "local");
    }

    #[test]
    fn default_os_returns_linux() {
        assert_eq!(default_os(), "linux");
    }

    #[test]
    fn default_true_returns_true() {
        assert!(default_true());
    }

    // -----------------------------------------------------------------------
    // machines_from_config
    // -----------------------------------------------------------------------

    fn make_machine_config(id: &str, name: &str, host: &str) -> MachineConfig {
        MachineConfig {
            id: id.to_string(),
            name: name.to_string(),
            host: host.to_string(),
            ip: None,
            os: "macos".to_string(),
            role: String::new(),
            gpu: None,
            enabled: true,
            tags: vec![],
            repos: vec![],
            home_dir: None,
        }
    }

    #[test]
    fn machines_from_config_returns_empty_map_for_empty_config() {
        let cfg = JarvisConfig::default();
        let map = machines_from_config(&cfg);
        assert!(map.is_empty());
    }

    #[test]
    fn machines_from_config_inserts_machines_by_id() {
        let cfg = JarvisConfig {
            session: Default::default(),
            machines: vec![
                make_machine_config("atlas", "ATLAS", "local"),
                make_machine_config("pixel", "PIXEL", "pixel"),
            ],
        };
        let map = machines_from_config(&cfg);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("atlas"));
        assert!(map.contains_key("pixel"));
    }

    #[test]
    fn machines_from_config_preserves_host_field() {
        let cfg = JarvisConfig {
            session: Default::default(),
            machines: vec![make_machine_config("pixel", "PIXEL", "pixel.tailnet")],
        };
        let map = machines_from_config(&cfg);
        assert_eq!(map["pixel"].host, "pixel.tailnet");
    }

    #[test]
    fn machines_from_config_first_repo_becomes_repo_path() {
        let cfg = JarvisConfig {
            session: Default::default(),
            machines: vec![MachineConfig {
                repos: vec![
                    RepoConfigToml { name: "myapp".to_string(), path: "/home/user/myapp".to_string(), github: String::new() },
                    RepoConfigToml { name: "other".to_string(), path: "/home/user/other".to_string(), github: String::new() },
                ],
                ..make_machine_config("m1", "M1", "local")
            }],
        };
        let map = machines_from_config(&cfg);
        let m = &map["m1"];
        assert_eq!(m.repo.as_deref(), Some("myapp"));
        assert_eq!(m.repo_path.as_deref(), Some("/home/user/myapp"));
        assert_eq!(m.repos.len(), 2);
    }

    #[test]
    fn machines_from_config_no_repos_gives_none_repo_path() {
        let cfg = JarvisConfig {
            session: Default::default(),
            machines: vec![make_machine_config("m1", "M1", "local")],
        };
        let map = machines_from_config(&cfg);
        assert!(map["m1"].repo.is_none());
        assert!(map["m1"].repo_path.is_none());
    }

    // -----------------------------------------------------------------------
    // TOML round-trip via JarvisConfig (no filesystem)
    // -----------------------------------------------------------------------

    #[test]
    fn jarvis_config_toml_round_trip() {
        let original = JarvisConfig {
            session: SessionConfig {
                id: "sess-1".to_string(),
                rama: "main".to_string(),
                objetivo: "build feature X".to_string(),
                native_notifications: Some(true),
                keywords_atlas: Some(vec!["build".to_string()]),
                keywords_pixel: None,
                budget_limit_usd: Some(5.0),
            },
            machines: vec![MachineConfig {
                id: "atlas".to_string(),
                name: "ATLAS".to_string(),
                host: "local".to_string(),
                ip: None,
                os: "macos".to_string(),
                role: "orchestrator".to_string(),
                gpu: None,
                enabled: true,
                tags: vec!["backend".to_string()],
                repos: vec![RepoConfigToml {
                    name: "myapp".to_string(),
                    path: "/home/user/myapp".to_string(),
                    github: "user/myapp".to_string(),
                }],
                home_dir: None,
            }],
        };

        let serialized = toml::to_string_pretty(&original).expect("serialize failed");
        let deserialized: JarvisConfig = toml::from_str(&serialized).expect("deserialize failed");

        assert_eq!(deserialized.session.id, "sess-1");
        assert_eq!(deserialized.session.rama, "main");
        assert_eq!(deserialized.session.budget_limit_usd, Some(5.0));
        assert_eq!(deserialized.machines.len(), 1);
        assert_eq!(deserialized.machines[0].id, "atlas");
        assert_eq!(deserialized.machines[0].repos[0].github, "user/myapp");
    }

    #[test]
    fn session_config_defaults_to_empty_strings() {
        let cfg: SessionConfig = toml::from_str("").expect("deserialize failed");
        assert_eq!(cfg.id, "");
        assert_eq!(cfg.rama, "");
        assert_eq!(cfg.objetivo, "");
        assert!(cfg.native_notifications.is_none());
        assert!(cfg.budget_limit_usd.is_none());
    }

    // -----------------------------------------------------------------------
    // run_fix_command
    // -----------------------------------------------------------------------

    #[test]
    fn test_run_fix_empty_command() {
        let result = run_fix_command("local".to_string(), "".to_string());
        assert!(result.is_err(), "empty command should return Err");
    }

    #[test]
    fn test_run_fix_too_long() {
        let long_cmd = "a".repeat(2001);
        let result = run_fix_command("local".to_string(), long_cmd);
        assert!(result.is_err(), "command > 2000 chars should return Err");
    }

    #[test]
    fn test_run_fix_unknown_machine() {
        let result = run_fix_command("nonexistent_xyz_123".to_string(), "echo hi".to_string());
        assert!(result.is_err(), "unknown machine_id should return Err");
        let msg = result.unwrap_err();
        assert!(
            msg.to_lowercase().contains("not found"),
            "error should mention 'not found', got: {}",
            msg
        );
    }

    // -----------------------------------------------------------------------
    // check_local_connections output structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_local_connections_returns_expected_checks() {
        let checks = check_local_connections();
        assert!(
            checks.len() >= 8,
            "expected at least 8 checks, got {}",
            checks.len()
        );
        let valid_statuses = ["ok", "error", "warning"];
        for check in &checks {
            assert!(
                !check.name.is_empty(),
                "check name must not be empty"
            );
            assert!(
                valid_statuses.contains(&check.status.as_str()),
                "check '{}' has unexpected status '{}'",
                check.name,
                check.status
            );
        }
    }

    #[test]
    fn test_check_local_connections_ssh_is_ok() {
        let checks = check_local_connections();
        let ssh = checks.iter().find(|c| c.name == "ssh")
            .expect("ssh check must be present");
        assert_eq!(ssh.status, "ok", "local ssh check should always be 'ok'");
    }

    #[test]
    fn test_check_local_connections_disk_has_detail() {
        let checks = check_local_connections();
        let disk = checks.iter().find(|c| c.name == "disk")
            .expect("disk check must be present");
        assert!(
            !disk.detail.is_empty(),
            "disk check should have a non-empty detail"
        );
    }

    // -----------------------------------------------------------------------
    // check_machine_connections with empty / invalid input
    // -----------------------------------------------------------------------

    #[test]
    fn test_check_machine_connections_empty_id() {
        let result = check_machine_connections("".to_string());
        assert_eq!(result.checks.len(), 1);
        let check = &result.checks[0];
        assert_eq!(check.name, "machine");
        assert_eq!(check.status, "error");
    }

    #[test]
    fn test_check_machine_connections_unknown_id() {
        let result = check_machine_connections("zzz_not_real".to_string());
        assert_eq!(result.checks.len(), 1);
        let check = &result.checks[0];
        assert_eq!(check.name, "machine");
        assert_eq!(check.status, "error");
    }
}
