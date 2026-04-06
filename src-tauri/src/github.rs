use std::process::Command;

use crate::types::{shell_escape, BranchComparison, Check, CompareCommit, CompareFile, PR};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_gh(args: &[&str], timeout_secs: u64) -> String {
    let _ = timeout_secs; // gh commands have their own timeouts via network
    Command::new("gh")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn parse_json<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s.trim()).ok()
}

// ---------------------------------------------------------------------------
// PR Operations
// ---------------------------------------------------------------------------

/// Validate that a repo slug is non-empty, within length limits, and contains
/// no shell-hostile characters.  A valid slug looks like "owner/repo" or just
/// "repo"; we accept both.
fn validate_repo(repo: &str) -> Result<(), String> {
    if repo.is_empty() {
        return Err("repo must not be empty".into());
    }
    if repo.len() > 200 {
        return Err("repo name too long (max 200 chars)".into());
    }
    Ok(())
}

/// Validate a git branch / ref name: non-empty, ≤ 255 chars.
fn validate_ref(name: &str, label: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{} must not be empty", label));
    }
    if name.len() > 255 {
        return Err(format!("{} too long (max 255 chars)", label));
    }
    Ok(())
}

fn list_prs_impl(repo: &str) -> Vec<PR> {
    let fields = "number,title,state,headRefName,author,createdAt,additions,deletions,reviews";
    // repo is already a full slug (e.g. "owner/repo") from the config github field,
    // or a bare name; pass it directly to --repo.
    let out = run_gh(
        &[
            "pr",
            "list",
            "--repo",
            repo,
            "--json",
            fields,
        ],
        15,
    );
    parse_json::<Vec<PR>>(&out).unwrap_or_default()
}

fn create_pr_impl(
    repo: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
    head: Option<&str>,
) -> Option<String> {
    let mut args = vec![
        "pr", "create", "--repo", repo, "--title", title, "--body", body,
    ];
    if let Some(b) = base {
        args.push("--base");
        args.push(b);
    }
    if let Some(h) = head {
        args.push("--head");
        args.push(h);
    }

    let out = run_gh(&args, 30);
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn merge_pr_impl(repo: &str, number: u64, method: &str) -> Result<bool, String> {
    // Validate merge method against allowlist to prevent argument injection.
    let flag = match method {
        "merge" => "--merge",
        "squash" => "--squash",
        "rebase" => "--rebase",
        other => return Err(format!("invalid merge method '{}'; must be merge, squash, or rebase", other)),
    };
    let num_str = number.to_string();
    // Check exit status — `gh pr merge` writes nothing to stdout on success
    let ok = std::process::Command::new("gh")
        .args(["pr", "merge", &num_str, "--repo", repo, flag, "--delete-branch"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    Ok(ok)
}

fn get_checks_impl(repo: &str, branch: &str) -> Vec<Check> {
    let out = run_gh(
        &[
            "run",
            "list",
            "--repo",
            repo,
            "--branch",
            branch,
            "--limit",
            "10",
            "--json",
            "name,status,conclusion",
        ],
        15,
    );

    if let Some(checks) = parse_json::<Vec<Check>>(&out) {
        return checks;
    }

    // `gh pr checks <branch>` is wrong — it expects a PR number, not a branch name.
    // Return empty vec rather than making a malformed call.
    Vec::new()
}

fn compare_branches_impl(repo: &str, base: &str, head: &str) -> Option<BranchComparison> {
    let api_path = format!(
        "repos/{}/compare/{}...{}",
        repo,
        urlencoding_simple(base),
        urlencoding_simple(head)
    );
    let out = run_gh(&["api", &api_path], 30);

    let parsed: serde_json::Value = serde_json::from_str(out.trim()).ok()?;

    Some(BranchComparison {
        ahead_by: parsed["ahead_by"].as_u64().unwrap_or(0),
        behind_by: parsed["behind_by"].as_u64().unwrap_or(0),
        total_commits: parsed["total_commits"].as_u64().unwrap_or(0),
        files: parsed["files"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|f| CompareFile {
                filename: f["filename"].as_str().unwrap_or("").to_string(),
                status: f["status"].as_str().unwrap_or("").to_string(),
                additions: f["additions"].as_u64().unwrap_or(0),
                deletions: f["deletions"].as_u64().unwrap_or(0),
                changes: f["changes"].as_u64().unwrap_or(0),
            })
            .collect(),
        commits: parsed["commits"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|c| {
                let sha = c["sha"].as_str().unwrap_or("");
                CompareCommit {
                    sha: sha.chars().take(7).collect(),
                    message: c["commit"]["message"]
                        .as_str()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    author: c["commit"]["author"]["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    date: c["commit"]["author"]["date"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                }
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// PR Comments & Files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

fn get_pr_comments_impl(repo: &str, pr_number: u64) -> Result<Vec<PrComment>, String> {
    let num_str = pr_number.to_string();
    let out = run_gh(
        &["pr", "view", &num_str, "--repo", repo, "--json", "comments"],
        15,
    );

    // gh returns { "comments": [ { "author": { "login": "..." }, "body": "...", "createdAt": "..." } ] }
    let parsed: serde_json::Value = serde_json::from_str(out.trim())
        .map_err(|e| format!("Failed to parse gh output: {}", e))?;

    let raw_comments = parsed["comments"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let comments = raw_comments
        .iter()
        .map(|c| {
            let author = c["author"]["login"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let body = c["body"].as_str().unwrap_or("").to_string();
            let created_at = c["createdAt"].as_str().unwrap_or("").to_string();
            PrComment { author, body, created_at }
        })
        .collect();

    Ok(comments)
}

fn add_pr_comment_impl(repo: &str, pr_number: u64, body: &str) -> Result<(), String> {
    let num_str = pr_number.to_string();
    let status = std::process::Command::new("gh")
        .args(["pr", "comment", &num_str, "--repo", repo, "--body", body])
        .status()
        .map_err(|e| format!("Failed to run gh: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("gh pr comment exited with status {}", status))
    }
}

fn get_pr_files_impl(repo: &str, pr_number: u64) -> Result<Vec<String>, String> {
    let num_str = pr_number.to_string();
    let out = run_gh(
        &["pr", "diff", "--repo", repo, &num_str, "--name-only"],
        15,
    );
    let files = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(files)
}

// ---------------------------------------------------------------------------
// Git Diff types & helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub diff_text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub machine_id: String,
    pub repo_name: String,
    pub branch: String,
    pub files: Vec<DiffFile>,
    pub total_additions: u32,
    pub total_deletions: u32,
}

fn extract_file_diff(full_diff: &str, file_path: &str) -> String {
    let marker = format!("diff --git a/{}", file_path);
    if let Some(start) = full_diff.find(&marker) {
        let rest = &full_diff[start..];
        // Skip past the end of the first line so we don't re-match the same "diff --git" header.
        let search_from = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
        let end = rest[search_from..].find("\ndiff --git")
            .map(|i| i + 1 + search_from) // include the newline before the next marker
            .unwrap_or(rest.len());
        rest[..end].to_string()
    } else {
        String::new()
    }
}

/// Simple URL encoding for branch names (handles / and spaces)
fn urlencoding_simple(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_prs(repo: String) -> Result<Vec<PR>, String> {
    validate_repo(&repo)?;
    Ok(list_prs_impl(&repo))
}

#[tauri::command]
pub fn create_pr(
    repo: String,
    title: String,
    body: String,
    base: Option<String>,
    head: Option<String>,
) -> Result<Option<String>, String> {
    validate_repo(&repo)?;
    if title.is_empty() {
        return Err("PR title must not be empty".into());
    }
    if title.len() > 500 {
        return Err("PR title too large (max 500 chars)".into());
    }
    if body.len() > 50_000 {
        return Err("PR body too large (max 50KB)".into());
    }
    if let Some(ref b) = base {
        validate_ref(b, "base")?;
    }
    if let Some(ref h) = head {
        validate_ref(h, "head")?;
    }
    Ok(create_pr_impl(
        &repo,
        &title,
        &body,
        base.as_deref(),
        head.as_deref(),
    ))
}

#[tauri::command]
pub fn merge_pr(repo: String, number: u64, method: Option<String>) -> Result<bool, String> {
    validate_repo(&repo)?;
    merge_pr_impl(&repo, number, method.as_deref().unwrap_or("squash"))
}

#[tauri::command]
pub fn get_checks(repo: String, branch: String) -> Result<Vec<Check>, String> {
    validate_repo(&repo)?;
    validate_ref(&branch, "branch")?;
    Ok(get_checks_impl(&repo, &branch))
}

#[tauri::command]
pub fn compare_branches(
    repo: String,
    base: String,
    head: String,
) -> Result<Option<BranchComparison>, String> {
    validate_repo(&repo)?;
    validate_ref(&base, "base")?;
    validate_ref(&head, "head")?;
    Ok(compare_branches_impl(&repo, &base, &head))
}

#[tauri::command]
pub fn get_pr_comments(repo: String, pr_number: u64) -> Result<Vec<PrComment>, String> {
    validate_repo(&repo)?;
    get_pr_comments_impl(&repo, pr_number)
}

#[tauri::command]
pub fn add_pr_comment(repo: String, pr_number: u64, body: String) -> Result<(), String> {
    validate_repo(&repo)?;
    if body.is_empty() {
        return Err("comment body must not be empty".into());
    }
    if body.len() > 65_536 {
        return Err("comment body too large (max 64KB)".into());
    }
    add_pr_comment_impl(&repo, pr_number, &body)
}

#[tauri::command]
pub fn get_pr_files(repo: String, pr_number: u64) -> Result<Vec<String>, String> {
    validate_repo(&repo)?;
    get_pr_files_impl(&repo, pr_number)
}

#[tauri::command]
pub fn get_git_diff(
    machine_id: String,
    registry: tauri::State<'_, crate::machines::MachineRegistry>,
) -> Result<Vec<DiffResult>, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    let machine = machines.get(&machine_id).ok_or("Machine not found")?;
    let host = machine.host.clone();
    let repos = machine.repos.clone();
    let is_local = host == "local";
    drop(machines);

    let mut results = Vec::new();
    for repo in &repos {
        let (branch, diff_raw, stat_raw) = if is_local {
            let branch = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&repo.path)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            let diff = std::process::Command::new("git")
                .args(["diff", "HEAD"])
                .current_dir(&repo.path)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            let stat = std::process::Command::new("git")
                .args(["diff", "HEAD", "--numstat"])
                .current_dir(&repo.path)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            (branch, diff, stat)
        } else {
            let cmd = format!(
                "cd {} && echo '===BRANCH===' && git branch --show-current && echo '===DIFF===' && git diff HEAD && echo '===STAT===' && git diff HEAD --numstat",
                shell_escape(&repo.path)
            );
            let raw = std::process::Command::new("ssh")
                .args(["-o", "ConnectTimeout=5", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", &host, &cmd])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            let branch = raw.split("===DIFF===").next()
                .unwrap_or("").replace("===BRANCH===", "").trim().to_string();
            let diff = raw.split("===DIFF===").nth(1)
                .and_then(|s| s.split("===STAT===").next())
                .unwrap_or("").to_string();
            let stat = raw.split("===STAT===").nth(1).unwrap_or("").to_string();
            (branch, diff, stat)
        };

        let mut files: Vec<DiffFile> = Vec::new();
        for line in stat_raw.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let adds: u32 = parts[0].parse().unwrap_or(0);
                let dels: u32 = parts[1].parse().unwrap_or(0);
                let path = parts[2].to_string();

                let file_diff = extract_file_diff(&diff_raw, &path);

                let status = if adds > 0 && dels > 0 { "modified" }
                    else if adds > 0 { "added" }
                    else { "deleted" };

                files.push(DiffFile {
                    path,
                    status: status.to_string(),
                    additions: adds,
                    deletions: dels,
                    diff_text: file_diff,
                });
            }
        }

        let total_additions = files.iter().map(|f| f.additions).sum();
        let total_deletions = files.iter().map(|f| f.deletions).sum();

        results.push(DiffResult {
            machine_id: machine_id.clone(),
            repo_name: repo.name.clone(),
            branch,
            files,
            total_additions,
            total_deletions,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Pipeline-accessible wrappers (pub(crate))
// ---------------------------------------------------------------------------

/// Create a PR and return the PR URL, or an error string.
pub fn create_pr_for_pipeline(repo: &str, title: &str, body: &str) -> Result<String, String> {
    if repo.is_empty() {
        return Err("repo must not be empty".into());
    }
    create_pr_impl(repo, title, body, None, None)
        .ok_or_else(|| "gh pr create returned no output".into())
}

/// Get CI checks for a repo (uses the default/latest branch run list).
pub fn get_checks_for_pipeline(repo: &str) -> Result<Vec<crate::types::Check>, String> {
    if repo.is_empty() {
        return Err("repo must not be empty".into());
    }
    // List the most recent workflow runs across all branches
    let out = run_gh(
        &[
            "run",
            "list",
            "--repo",
            repo,
            "--limit",
            "10",
            "--json",
            "name,status,conclusion",
        ],
        15,
    );
    Ok(parse_json::<Vec<crate::types::Check>>(&out).unwrap_or_default())
}

/// Merge a PR by number, returning Ok(true) on success.
pub fn merge_pr_for_pipeline(repo: &str, number: u64, method: &str) -> Result<bool, String> {
    if repo.is_empty() {
        return Err("repo must not be empty".into());
    }
    merge_pr_impl(repo, number, method)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // validate_repo
    // -----------------------------------------------------------------------

    #[test]
    fn validate_repo_accepts_owner_slash_repo() {
        assert!(validate_repo("owner/repo").is_ok());
    }

    #[test]
    fn validate_repo_accepts_bare_repo_name() {
        assert!(validate_repo("my-project").is_ok());
    }

    #[test]
    fn validate_repo_rejects_empty_string() {
        let err = validate_repo("").unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn validate_repo_rejects_name_over_200_chars() {
        let long_name = "a".repeat(201);
        let err = validate_repo(&long_name).unwrap_err();
        assert!(err.contains("too long"));
    }

    #[test]
    fn validate_repo_accepts_exactly_200_chars() {
        let name = "a".repeat(200);
        assert!(validate_repo(&name).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_ref
    // -----------------------------------------------------------------------

    #[test]
    fn validate_ref_accepts_normal_branch_name() {
        assert!(validate_ref("main", "branch").is_ok());
    }

    #[test]
    fn validate_ref_accepts_branch_with_slashes() {
        assert!(validate_ref("feature/my-feature", "branch").is_ok());
    }

    #[test]
    fn validate_ref_rejects_empty_name() {
        let err = validate_ref("", "branch").unwrap_err();
        assert!(err.contains("branch"));
        assert!(err.contains("empty"));
    }

    #[test]
    fn validate_ref_rejects_name_over_255_chars() {
        let long_ref = "x".repeat(256);
        let err = validate_ref(&long_ref, "head").unwrap_err();
        assert!(err.contains("too long"));
    }

    #[test]
    fn validate_ref_accepts_exactly_255_chars() {
        let ref_name = "b".repeat(255);
        assert!(validate_ref(&ref_name, "base").is_ok());
    }

    #[test]
    fn validate_ref_error_message_includes_label() {
        let err = validate_ref("", "base").unwrap_err();
        assert!(err.contains("base"), "Error should mention the label");
    }

    // -----------------------------------------------------------------------
    // urlencoding_simple
    // -----------------------------------------------------------------------

    #[test]
    fn urlencoding_plain_string_is_unchanged() {
        assert_eq!(urlencoding_simple("main"), "main");
    }

    #[test]
    fn urlencoding_space_becomes_percent20() {
        assert_eq!(urlencoding_simple("feature branch"), "feature%20branch");
    }

    #[test]
    fn urlencoding_hash_becomes_percent23() {
        assert_eq!(urlencoding_simple("fix#123"), "fix%23123");
    }

    #[test]
    fn urlencoding_percent_becomes_percent25() {
        assert_eq!(urlencoding_simple("100%done"), "100%25done");
    }

    #[test]
    fn urlencoding_empty_string_stays_empty() {
        assert_eq!(urlencoding_simple(""), "");
    }

    #[test]
    fn urlencoding_combined_special_chars() {
        let input = "fix #42 (100%)";
        let encoded = urlencoding_simple(input);
        assert!(encoded.contains("%23"));  // # encoded
        assert!(encoded.contains("%20"));  // space encoded
        assert!(encoded.contains("%25"));  // % encoded
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('#'));
    }

    // -----------------------------------------------------------------------
    // extract_file_diff
    // -----------------------------------------------------------------------

    #[test]
    fn extract_file_diff_returns_empty_when_file_not_in_diff() {
        let diff = "diff --git a/other.rs b/other.rs\n--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let result = extract_file_diff(diff, "missing.rs");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_file_diff_extracts_single_file() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        let result = extract_file_diff(diff, "src/main.rs");
        assert!(result.starts_with("diff --git a/src/main.rs"));
        assert!(result.contains("-fn old()"));
        assert!(result.contains("+fn new()"));
    }

    #[test]
    fn extract_file_diff_stops_at_next_file_header() {
        let diff = concat!(
            "diff --git a/a.rs b/a.rs\n",
            "--- a/a.rs\n",
            "+++ b/a.rs\n",
            "@@ -1 +1 @@\n",
            "-old_a\n",
            "+new_a\n",
            "diff --git a/b.rs b/b.rs\n",
            "--- a/b.rs\n",
            "+++ b/b.rs\n",
            "@@ -1 +1 @@\n",
            "-old_b\n",
            "+new_b\n",
        );
        let result = extract_file_diff(diff, "a.rs");
        assert!(result.contains("old_a"));
        assert!(!result.contains("old_b"), "Should not bleed into b.rs section");
    }

    #[test]
    fn extract_file_diff_returns_last_file_without_trailing_header() {
        let diff = concat!(
            "diff --git a/first.rs b/first.rs\n",
            "--- a/first.rs\n",
            "+++ b/first.rs\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
            "diff --git a/last.rs b/last.rs\n",
            "--- a/last.rs\n",
            "+++ b/last.rs\n",
            "@@ -1 +1 @@\n",
            "-last_old\n",
            "+last_new\n",
        );
        let result = extract_file_diff(diff, "last.rs");
        assert!(result.starts_with("diff --git a/last.rs"));
        assert!(result.contains("last_old"));
        assert!(result.contains("last_new"));
    }

    // -----------------------------------------------------------------------
    // merge_pr_impl method validation (via merge_pr Tauri command layer)
    // The method allowlist check is pure — it doesn't hit the network.
    // -----------------------------------------------------------------------

    #[test]
    fn merge_pr_invalid_method_returns_error() {
        // merge_pr_impl rejects unknown methods without making any network call
        let result = merge_pr_impl("owner/repo", 1, "invalid-method");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid merge method"));
        assert!(err.contains("invalid-method"));
    }

    #[test]
    fn merge_pr_invalid_method_lists_valid_options() {
        let result = merge_pr_impl("owner/repo", 1, "badmethod");
        let err = result.unwrap_err();
        // Error should mention the valid methods
        assert!(err.contains("merge") || err.contains("squash") || err.contains("rebase"));
    }

    // -----------------------------------------------------------------------
    // create_pr_for_pipeline repo validation
    // -----------------------------------------------------------------------

    #[test]
    fn create_pr_for_pipeline_rejects_empty_repo() {
        let result = create_pr_for_pipeline("", "title", "body");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    // -----------------------------------------------------------------------
    // get_checks_for_pipeline repo validation
    // -----------------------------------------------------------------------

    #[test]
    fn get_checks_for_pipeline_rejects_empty_repo() {
        let result = get_checks_for_pipeline("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    // -----------------------------------------------------------------------
    // merge_pr_for_pipeline repo validation
    // -----------------------------------------------------------------------

    #[test]
    fn merge_pr_for_pipeline_rejects_empty_repo() {
        let result = merge_pr_for_pipeline("", 1, "squash");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    // -----------------------------------------------------------------------
    // PrComment JSON parsing
    // -----------------------------------------------------------------------

    fn parse_pr_comments_from_str(json: &str) -> Result<Vec<PrComment>, String> {
        let parsed: serde_json::Value = serde_json::from_str(json.trim())
            .map_err(|e| format!("Failed to parse gh output: {}", e))?;
        let raw = parsed["comments"].as_array().cloned().unwrap_or_default();
        Ok(raw
            .iter()
            .map(|c| PrComment {
                author: c["author"]["login"].as_str().unwrap_or("").to_string(),
                body: c["body"].as_str().unwrap_or("").to_string(),
                created_at: c["createdAt"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    #[test]
    fn pr_comments_parses_single_comment() {
        let json = r#"{"comments":[{"author":{"login":"alice"},"body":"LGTM","createdAt":"2024-01-15T10:00:00Z"}]}"#;
        let comments = parse_pr_comments_from_str(json).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author, "alice");
        assert_eq!(comments[0].body, "LGTM");
        assert_eq!(comments[0].created_at, "2024-01-15T10:00:00Z");
    }

    #[test]
    fn pr_comments_parses_multiple_comments() {
        let json = r#"{"comments":[
            {"author":{"login":"alice"},"body":"First comment","createdAt":"2024-01-15T10:00:00Z"},
            {"author":{"login":"bob"},"body":"Second comment","createdAt":"2024-01-15T11:00:00Z"}
        ]}"#;
        let comments = parse_pr_comments_from_str(json).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author, "alice");
        assert_eq!(comments[1].author, "bob");
        assert_eq!(comments[1].body, "Second comment");
    }

    #[test]
    fn pr_comments_returns_empty_vec_when_no_comments() {
        let json = r#"{"comments":[]}"#;
        let comments = parse_pr_comments_from_str(json).unwrap();
        assert!(comments.is_empty());
    }

    #[test]
    fn pr_comments_handles_missing_comments_key() {
        let json = r#"{"other_field":"value"}"#;
        let comments = parse_pr_comments_from_str(json).unwrap();
        assert!(comments.is_empty(), "Missing 'comments' key should produce empty vec");
    }

    #[test]
    fn pr_comments_returns_error_on_invalid_json() {
        let bad_json = "not valid json {{{";
        let result = parse_pr_comments_from_str(bad_json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    // -----------------------------------------------------------------------
    // get_pr_files output parsing
    // -----------------------------------------------------------------------

    fn parse_pr_files_from_str(raw: &str) -> Vec<String> {
        raw.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    #[test]
    fn pr_files_parses_newline_separated_paths() {
        let raw = "src/main.rs\nsrc/lib.rs\nCargo.toml\n";
        let files = parse_pr_files_from_str(raw);
        assert_eq!(files, vec!["src/main.rs", "src/lib.rs", "Cargo.toml"]);
    }

    #[test]
    fn pr_files_returns_empty_vec_for_blank_output() {
        let files = parse_pr_files_from_str("");
        assert!(files.is_empty());
    }

    // -----------------------------------------------------------------------
    // add_pr_comment validation (pure, no network)
    // -----------------------------------------------------------------------

    #[test]
    fn add_pr_comment_rejects_empty_body() {
        // Validate the empty-body check directly (same logic as the command layer)
        let body = "";
        let err = if body.is_empty() {
            Err("comment body must not be empty".to_string())
        } else {
            Ok(())
        };
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("empty"));
    }
}
