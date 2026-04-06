use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::Command;

use crate::machines::MachineRegistry;
use crate::types::shell_escape;

// ---------------------------------------------------------------------------
// ConflictResolution enum
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictResolution {
    Ours,
    Theirs,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictReport {
    pub machine_a: String,
    pub machine_b: String,
    pub repo: String,
    pub overlapping_files: Vec<String>,
    pub branch_a: String,
    pub branch_b: String,
    pub detected_at: String,
}

/// Get the current branch and list of files changed vs main for a local repo.
/// Returns (branch, changed_files). Returns empty strings/vec on error.
fn get_local_branch_and_files(repo_path: &str) -> (String, Vec<String>) {
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if branch.is_empty() || branch == "main" || branch == "master" {
        return (branch, Vec::new());
    }

    // Try main first, fall back to master for repos using the older default branch name
    let files_out = ["main", "master"].iter().find_map(|base| {
        let out = Command::new("git")
            .args(["diff", "--name-only", &format!("{base}...HEAD")])
            .current_dir(repo_path)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if out.trim().is_empty() { None } else { Some(out) }
    }).unwrap_or_default();

    let files: Vec<String> = files_out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    (branch, files)
}

/// Get the current branch and list of files changed vs main for a remote repo via SSH.
/// Returns (branch, changed_files). Returns empty strings/vec on error or if on main.
fn get_remote_branch_and_files(host: &str, repo_path: &str) -> (String, Vec<String>) {
    let escaped = shell_escape(repo_path);
    // Combine both git commands in a single SSH call, separated by ===SECTION===
    let script = format!(
        "cd {escaped} && git rev-parse --abbrev-ref HEAD && echo '===SECTION===' && (git diff --name-only main...HEAD 2>/dev/null || git diff --name-only master...HEAD 2>/dev/null || true)"
    );

    let raw = Command::new("ssh")
        .args([
            "-o", "ConnectTimeout=5",
            "-o", "ServerAliveInterval=5",
            "-o", "ServerAliveCountMax=3",
            host,
            &script,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if let Some((branch_part, files_part)) = raw.split_once("===SECTION===") {
        let branch = branch_part.trim().to_string();
        if branch.is_empty() || branch == "main" || branch == "master" {
            return (branch, Vec::new());
        }
        let files: Vec<String> = files_part
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        (branch, files)
    } else {
        (String::new(), Vec::new())
    }
}

#[tauri::command]
pub fn detect_conflicts(
    registry: tauri::State<'_, MachineRegistry>,
) -> Vec<ConflictReport> {
    // Snapshot needed data without holding the lock during I/O
    let machine_repos: Vec<(String, String, String, String)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines
            .values()
            .filter(|m| m.enabled)
            .flat_map(|m| {
                m.repos.iter().map(|r| {
                    (m.id.clone(), m.host.clone(), r.name.clone(), r.path.clone())
                })
                .collect::<Vec<_>>()
            })
            .collect()
    };

    // Fetch branch+files for each (machine, repo) in parallel
    let handles: Vec<_> = machine_repos
        .iter()
        .map(|(machine_id, host, repo_name, repo_path)| {
            let machine_id = machine_id.clone();
            let host = host.clone();
            let repo_name = repo_name.clone();
            let repo_path = repo_path.clone();
            std::thread::spawn(move || {
                let (branch, files) = if host == "local" {
                    get_local_branch_and_files(&repo_path)
                } else {
                    get_remote_branch_and_files(&host, &repo_path)
                };
                (machine_id, repo_name, branch, files)
            })
        })
        .collect();

    let mut results: Vec<(String, String, String, Vec<String>)> = Vec::new();
    for handle in handles {
        if let Ok(r) = handle.join() {
            results.push(r);
        }
    }

    // Group results by repo name
    let mut by_repo: std::collections::HashMap<String, Vec<(String, String, Vec<String>)>> =
        std::collections::HashMap::new();
    for (machine_id, repo_name, branch, files) in results {
        by_repo
            .entry(repo_name)
            .or_default()
            .push((machine_id, branch, files));
    }

    // For each repo with 2+ machines on different non-main branches, check file overlap
    let mut reports = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for (repo_name, entries) in by_repo {
        // Only care about repos where multiple machines are active
        if entries.len() < 2 {
            continue;
        }

        // Check all pairs
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let (ref machine_a, ref branch_a, ref files_a) = entries[i];
                let (ref machine_b, ref branch_b, ref files_b) = entries[j];

                // Skip if either is on main/master or has no changed files
                if branch_a.is_empty()
                    || branch_b.is_empty()
                    || branch_a == "main"
                    || branch_a == "master"
                    || branch_b == "main"
                    || branch_b == "master"
                {
                    continue;
                }

                // Skip if on the same branch (no conflict)
                if branch_a == branch_b {
                    continue;
                }

                if files_a.is_empty() || files_b.is_empty() {
                    continue;
                }

                let set_a: HashSet<&String> = files_a.iter().collect();
                let overlapping: Vec<String> = files_b
                    .iter()
                    .filter(|f| set_a.contains(f))
                    .cloned()
                    .collect();

                if !overlapping.is_empty() {
                    reports.push(ConflictReport {
                        machine_a: machine_a.clone(),
                        machine_b: machine_b.clone(),
                        repo: repo_name.clone(),
                        overlapping_files: overlapping,
                        branch_a: branch_a.clone(),
                        branch_b: branch_b.clone(),
                        detected_at: now.clone(),
                    });
                }
            }
        }
    }

    reports
}

// ---------------------------------------------------------------------------
// resolve_conflict
// ---------------------------------------------------------------------------

/// Resolve a single conflicted file using the given strategy ("ours" or "theirs"),
/// then stage it with `git add`. "manual" is a no-op (caller handles resolution).
#[tauri::command]
pub fn resolve_conflict(
    repo_path: String,
    file: String,
    resolution: String,
) -> Result<String, String> {
    if repo_path.is_empty() {
        return Err("repo_path must not be empty".into());
    }
    if file.is_empty() {
        return Err("file must not be empty".into());
    }

    // Validate the resolution string up-front
    let strategy = resolution.to_lowercase();
    match strategy.as_str() {
        "ours" | "theirs" => {}
        "manual" => return Ok(format!("manual: no auto-resolution applied to {}", file)),
        other => return Err(format!("unknown resolution '{}': expected ours, theirs, or manual", other)),
    }

    // git checkout --ours/--theirs <file>
    let checkout = Command::new("git")
        .args(["checkout", &format!("--{}", strategy), "--", &file])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to run git checkout: {}", e))?;

    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr).trim().to_string();
        return Err(format!("git checkout --{} failed: {}", strategy, stderr));
    }

    // git add <file>
    let add = Command::new("git")
        .args(["add", "--", &file])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to run git add: {}", e))?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr).trim().to_string();
        return Err(format!("git add failed: {}", stderr));
    }

    Ok(format!("resolved {} with strategy '{}'", file, strategy))
}

// ---------------------------------------------------------------------------
// auto_resolve_conflicts
// ---------------------------------------------------------------------------

/// Find all files currently in a conflicted state (unmerged) in the given repo
/// and resolve each one using the "ours" strategy. Returns the list of resolved files.
#[tauri::command]
pub fn auto_resolve_conflicts(repo_path: String) -> Result<Vec<String>, String> {
    if repo_path.is_empty() {
        return Err("repo_path must not be empty".into());
    }

    // List unmerged (conflicted) files
    let out = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to run git diff: {}", e))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("git diff --diff-filter=U failed: {}", stderr));
    }

    let conflicted_files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if conflicted_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut resolved = Vec::new();
    let mut errors = Vec::new();

    for file in &conflicted_files {
        match resolve_conflict(repo_path.clone(), file.clone(), "ours".into()) {
            Ok(_) => resolved.push(file.clone()),
            Err(e) => errors.push(format!("{}: {}", file, e)),
        }
    }

    if !errors.is_empty() && resolved.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(resolved)
}

// ---------------------------------------------------------------------------
// get_conflict_diff
// ---------------------------------------------------------------------------

/// Return the raw file content (including conflict markers) for a conflicted file.
#[tauri::command]
pub fn get_conflict_diff(repo_path: String, file: String) -> Result<String, String> {
    if repo_path.is_empty() {
        return Err("repo_path must not be empty".into());
    }
    if file.is_empty() {
        return Err("file must not be empty".into());
    }

    let path = std::path::Path::new(&repo_path).join(&file);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {}", file, e))?;

    Ok(content)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Create a unique temp directory under the system temp dir.
    fn make_temp_dir(suffix: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("jarvis_conflicts_test_{}_{}", suffix, std::process::id()));
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// Create a temporary git repo with an ongoing merge conflict in `file.txt`.
    /// Returns the path to the directory (caller is responsible for cleanup).
    fn make_conflict_repo(suffix: &str) -> PathBuf {
        let path = make_temp_dir(suffix);

        // Init repo and configure identity so commits work
        Command::new("git").args(["init"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["checkout", "-b", "main"]).current_dir(&path).output().unwrap();

        // Initial commit on main
        fs::write(path.join("file.txt"), "line1\nline2\nline3\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&path).output().unwrap();

        // Create feature branch and make a conflicting change
        Command::new("git").args(["checkout", "-b", "feature"]).current_dir(&path).output().unwrap();
        fs::write(path.join("file.txt"), "line1\nFEATURE\nline3\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git").args(["commit", "-m", "feature change"]).current_dir(&path).output().unwrap();

        // Back to main and make a conflicting change on the same line
        Command::new("git").args(["checkout", "main"]).current_dir(&path).output().unwrap();
        fs::write(path.join("file.txt"), "line1\nMAIN\nline3\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git").args(["commit", "-m", "main change"]).current_dir(&path).output().unwrap();

        // Merge feature → main (will conflict); exit code is non-zero on conflict — expected
        Command::new("git").args(["merge", "--no-ff", "feature"]).current_dir(&path).output().unwrap();

        path
    }

    // -----------------------------------------------------------------------
    // resolve_conflict
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_conflict_empty_repo_path_returns_err() {
        let result = resolve_conflict("".into(), "file.txt".into(), "ours".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("repo_path"));
    }

    #[test]
    fn resolve_conflict_empty_file_returns_err() {
        let result = resolve_conflict("/tmp".into(), "".into(), "ours".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file"));
    }

    #[test]
    fn resolve_conflict_invalid_resolution_returns_err() {
        let result = resolve_conflict("/tmp".into(), "file.txt".into(), "both".into());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("unknown resolution"), "got: {}", msg);
    }

    #[test]
    fn resolve_conflict_manual_is_noop() {
        let result = resolve_conflict("/tmp".into(), "file.txt".into(), "manual".into());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("manual"));
    }

    #[test]
    fn resolve_conflict_ours_resolves_conflict() {
        let repo = make_conflict_repo("resolve_ours");

        let result = resolve_conflict(repo.to_str().unwrap().to_string(), "file.txt".into(), "ours".into());
        assert!(result.is_ok(), "resolve_conflict failed: {:?}", result.err());

        // After resolution the file should contain the "ours" (main) version
        let content = fs::read_to_string(repo.join("file.txt")).unwrap();
        assert!(content.contains("MAIN"), "expected 'MAIN' after ours resolution, got: {}", content);
        assert!(!content.contains("<<<<<<<"), "conflict markers should be gone");

        let _ = fs::remove_dir_all(&repo);
    }

    // -----------------------------------------------------------------------
    // auto_resolve_conflicts
    // -----------------------------------------------------------------------

    #[test]
    fn auto_resolve_conflicts_resolves_all_files() {
        let repo = make_conflict_repo("auto_resolve");

        let result = auto_resolve_conflicts(repo.to_str().unwrap().to_string());
        assert!(result.is_ok(), "auto_resolve_conflicts failed: {:?}", result.err());

        let resolved = result.unwrap();
        assert_eq!(resolved, vec!["file.txt".to_string()]);

        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn auto_resolve_conflicts_empty_repo_path_returns_err() {
        let result = auto_resolve_conflicts("".into());
        assert!(result.is_err());
    }

    #[test]
    fn auto_resolve_conflicts_clean_repo_returns_empty_vec() {
        let path = make_temp_dir("clean_repo");
        Command::new("git").args(["init"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&path).output().unwrap();
        Command::new("git").args(["checkout", "-b", "main"]).current_dir(&path).output().unwrap();
        fs::write(path.join("readme.txt"), "hello\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&path).output().unwrap();

        let result = auto_resolve_conflicts(path.to_str().unwrap().to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());

        let _ = fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // get_conflict_diff
    // -----------------------------------------------------------------------

    #[test]
    fn get_conflict_diff_returns_conflict_markers() {
        let repo = make_conflict_repo("conflict_diff");

        let result = get_conflict_diff(repo.to_str().unwrap().to_string(), "file.txt".into());
        assert!(result.is_ok(), "get_conflict_diff failed: {:?}", result.err());
        let content = result.unwrap();
        assert!(
            content.contains("<<<<<<<"),
            "expected conflict markers in diff, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn get_conflict_diff_missing_file_returns_err() {
        let path = make_temp_dir("missing_file");
        let result = get_conflict_diff(path.to_str().unwrap().to_string(), "nonexistent.txt".into());
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn get_conflict_diff_empty_repo_path_returns_err() {
        let result = get_conflict_diff("".into(), "file.txt".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("repo_path"));
    }

    // -----------------------------------------------------------------------
    // ConflictResolution serialization
    // -----------------------------------------------------------------------

    #[test]
    fn conflict_resolution_serializes_correctly() {
        let ours = ConflictResolution::Ours;
        let theirs = ConflictResolution::Theirs;
        let manual = ConflictResolution::Manual;
        assert_eq!(serde_json::to_string(&ours).unwrap(), "\"ours\"");
        assert_eq!(serde_json::to_string(&theirs).unwrap(), "\"theirs\"");
        assert_eq!(serde_json::to_string(&manual).unwrap(), "\"manual\"");
    }

    #[test]
    fn conflict_resolution_deserializes_correctly() {
        let ours: ConflictResolution = serde_json::from_str("\"ours\"").unwrap();
        let theirs: ConflictResolution = serde_json::from_str("\"theirs\"").unwrap();
        assert!(matches!(ours, ConflictResolution::Ours));
        assert!(matches!(theirs, ConflictResolution::Theirs));
    }
}
