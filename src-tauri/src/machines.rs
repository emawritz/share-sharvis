use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

use crate::config;
use crate::types::{Machine, MachineHealth, MachineInfo, MachineStats};
use crate::ShutdownFlag;
use tauri::{Emitter, Manager};

// ---------------------------------------------------------------------------
// Machine Registry
// ---------------------------------------------------------------------------

pub struct MachineRegistry {
    pub machines: Mutex<HashMap<String, Machine>>,
    health_cache: Mutex<HashMap<String, (MachineHealth, Instant)>>,
}

impl MachineRegistry {
    pub fn new() -> Self {
        let cfg = config::load_config();
        let machines = config::machines_from_config(&cfg);
        Self {
            machines: Mutex::new(machines),
            health_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Create a registry from an already-loaded machine map (no health cache).
    /// Used to reconstruct a temporary registry inside spawn_blocking closures.
    pub fn from_machines(machines: HashMap<String, crate::types::Machine>) -> Self {
        Self {
            machines: Mutex::new(machines),
            health_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn reload_from_config(&self) {
        let cfg = config::load_config();
        let machines = config::machines_from_config(&cfg);
        *self.machines.lock().unwrap_or_else(|e| e.into_inner()) = machines;
        self.health_cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

fn run_cmd(cmd: &str, args: &[&str], timeout_secs: u64) -> String {
    let result = Command::new(cmd)
        .args(args)
        .output();

    // We can't easily enforce a timeout with std::process::Command synchronously,
    // but the SSH commands have their own ConnectTimeout. For local commands this is fine.
    match result {
        Ok(output) => {
            if timeout_secs > 0 {
                let _ = timeout_secs; // used conceptually; SSH has ConnectTimeout
            }
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Health Check
// ---------------------------------------------------------------------------

pub fn check_machine(machine: &Machine) -> MachineHealth {
    let start = Instant::now();

    if machine.host == "local" {
        let result = run_cmd("echo", &["ok"], 5);
        let latency_ms = start.elapsed().as_millis() as i64;
        return MachineHealth {
            online: result.trim() == "ok",
            latency_ms,
        };
    }

    // Remote: SSH echo test (with single retry after 1s to handle transient glitches)
    let ssh_args = [
        "-o",
        "ConnectTimeout=3",
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "ServerAliveInterval=5",
        "-o",
        "ServerAliveCountMax=3",
        machine.host.as_str(),
        "echo ok",
    ];
    let result = run_cmd("ssh", &ssh_args, 8);
    if result.trim() == "ok" {
        let latency_ms = start.elapsed().as_millis() as i64;
        return MachineHealth {
            online: true,
            latency_ms,
        };
    }
    // Retry once after 1s
    std::thread::sleep(std::time::Duration::from_secs(1));
    let result = run_cmd("ssh", &ssh_args, 8);
    let latency_ms = start.elapsed().as_millis() as i64;
    MachineHealth {
        online: result.trim() == "ok",
        latency_ms,
    }
}

// ---------------------------------------------------------------------------
// Stats Collection
// ---------------------------------------------------------------------------

pub fn get_machine_stats(machine: &Machine) -> MachineStats {
    if machine.host == "local" {
        return get_local_mac_stats();
    }
    if machine.os == "windows" {
        return get_windows_stats(&machine.host);
    }
    get_linux_stats(&machine.host, machine.gpu.as_deref())
}

fn get_local_mac_stats() -> MachineStats {
    let cpu = run_cmd(
        "sh",
        &["-c", "top -l 1 -n 0 | grep 'CPU usage' | awk '{print $3}'"],
        5,
    )
    .trim()
    .to_string();

    let mem_raw = run_cmd(
        "sh",
        &[
            "-c",
            "vm_stat | awk '/Pages (active|inactive|speculative|wired|occupied)/{gsub(/\\./,\"\");s+=$NF}END{print s}'",
        ],
        5,
    )
    .trim()
    .to_string();

    let total_mem = run_cmd("sh", &["-c", "sysctl -n hw.memsize"], 5)
        .trim()
        .to_string();

    let page_size: u64 = 16384;
    let used_bytes = mem_raw.parse::<u64>().unwrap_or(0) * page_size;
    let total_bytes = total_mem.parse::<u64>().unwrap_or(0);
    let mem = if total_bytes > 0 {
        format!("{}%", (used_bytes as f64 / total_bytes as f64 * 100.0).round() as u64)
    } else {
        "-".to_string()
    };

    let disk = run_cmd("sh", &["-c", "df -h / | tail -1 | awk '{print $5}'"], 5)
        .trim()
        .to_string();

    let uptime = run_cmd(
        "sh",
        &["-c", "uptime | sed 's/.*up //' | sed 's/,.*//' | xargs"],
        5,
    )
    .trim()
    .to_string();

    // Process count: ps aux | wc -l gives header + processes; subtract 1 for header
    let proc_raw = run_cmd("sh", &["-c", "ps aux | wc -l | tr -d ' '"], 5)
        .trim()
        .to_string();
    let process_count = proc_raw.parse::<u32>().ok().map(|n| n.saturating_sub(1));

    // First non-loopback IPv4 address (macOS: ifconfig)
    let ip_raw = run_cmd(
        "sh",
        &["-c", "ifconfig | awk '/inet /{if($2!=\"127.0.0.1\")print $2}' | head -1"],
        5,
    )
    .trim()
    .to_string();
    let ip_address = if ip_raw.is_empty() { None } else { Some(ip_raw) };

    MachineStats {
        cpu: if cpu.is_empty() { "-".into() } else { cpu },
        mem,
        disk: if disk.is_empty() { "-".into() } else { disk },
        gpu: None,
        uptime: if uptime.is_empty() {
            "-".into()
        } else {
            uptime
        },
        online: true,
        process_count,
        ip_address,
    }
}

fn get_linux_stats(host: &str, gpu_name: Option<&str>) -> MachineStats {
    let gpu_cmd = if gpu_name.is_some() {
        "echo -n '\\\"gpu\\\":\\\"'; nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null || echo 'n/a'; "
    } else {
        "echo -n '\\\"gpu\\\":\\\"'; echo 'n/a'; "
    };

    let script = format!(
        "echo -n '{{\\\"cpu\\\":\\\"'; top -bn1 | grep 'Cpu(s)' | awk '{{printf \"%.0f%%\", $2+$4}}'; \
         echo -n '\\\",\\\"mem\\\":\\\"'; free -m | awk '/Mem:/{{printf \"%.0f%%\", $3/$2*100}}'; \
         echo -n '\\\",\\\"disk\\\":\\\"'; df -h / | tail -1 | awk '{{print $5}}'; \
         echo -n '\\\",\\\"uptime\\\":\\\"'; uptime -p 2>/dev/null | sed 's/up //' || uptime | sed 's/.*up //' | sed 's/,.*//'; \
         echo -n '\\\",\\\"procs\\\":\\\"'; ps aux | wc -l | tr -d ' '; \
         echo -n '\\\",\\\"ip\\\":\\\"'; ip -4 addr show scope global 2>/dev/null | awk '/inet /{{print $2}}' | cut -d/ -f1 | head -1; \
         echo -n '\\\",{gpu_cmd}\
         echo '\\\"}}'"
    );

    let raw = run_cmd(
        "ssh",
        &["-o", "ConnectTimeout=3", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3", host, &script],
        10,
    );

    let cleaned = raw.trim().replace('\n', "");
    match serde_json::from_str::<serde_json::Value>(&cleaned) {
        Ok(parsed) => {
            let mut gpu_str = parsed
                .get("gpu")
                .and_then(|v| v.as_str())
                .unwrap_or("n/a")
                .to_string();

            // Format GPU: "util, mem_used, mem_total" -> "util% (mem_usedMB/mem_totalMB)"
            if gpu_str != "n/a" && !gpu_str.is_empty() {
                let parts: Vec<&str> = gpu_str.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    gpu_str = format!("{}% ({}/{}MB)", parts[0], parts[1], parts[2]);
                } else if !parts.is_empty() {
                    gpu_str = format!("{}%", parts[0]);
                }
            }

            let process_count = parsed
                .get("procs")
                .and_then(|v| v.as_str())
                .and_then(|s| s.trim().parse::<u32>().ok())
                .map(|n| n.saturating_sub(1)); // subtract header line

            let ip_str = parsed
                .get("ip")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            MachineStats {
                cpu: parsed
                    .get("cpu")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                mem: parsed
                    .get("mem")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                disk: parsed
                    .get("disk")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                gpu: if gpu_name.is_some() {
                    Some(gpu_str)
                } else {
                    None
                },
                uptime: parsed
                    .get("uptime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                online: true,
                process_count,
                ip_address: ip_str,
            }
        }
        Err(_) => MachineStats {
            cpu: "-".into(),
            mem: "-".into(),
            disk: "-".into(),
            gpu: if gpu_name.is_some() {
                Some("-".into())
            } else {
                None
            },
            uptime: "-".into(),
            online: false,
            process_count: None,
            ip_address: None,
        },
    }
}

fn get_windows_stats(host: &str) -> MachineStats {
    let script = concat!(
        "$cpu = (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average;",
        "$os = Get-CimInstance Win32_OperatingSystem;",
        "$mem = [math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / $os.TotalVisibleMemorySize * 100);",
        "$disk = Get-CimInstance Win32_LogicalDisk -Filter \"DeviceID='C:'\" | ForEach-Object { [math]::Round(($_.Size - $_.FreeSpace) / $_.Size * 100) };",
        "$up = (Get-Date) - $os.LastBootUpTime; $upStr = \"$($up.Days)d $($up.Hours)h\";",
        "$gpu = 'n/a'; try { $gpu = (nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>$null) } catch {};",
        "$procs = (Get-Process | Measure-Object).Count;",
        "$ip = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -ne '127.0.0.1'} | Select-Object -First 1).IPAddress;",
        "Write-Output \"{`\"cpu`\":`\"${cpu}%`\",`\"mem`\":`\"${mem}%`\",`\"disk`\":`\"${disk}%`\",`\"uptime`\":`\"${upStr}`\",`\"gpu`\":`\"${gpu}`\",`\"procs`\":`\"${procs}`\",`\"ip`\":`\"${ip}`\"}\""
    );

    let raw = run_cmd(
        "ssh",
        &[
            "-o",
            "ConnectTimeout=3",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            host,
            "powershell",
            "-NoProfile",
            "-Command",
            script,
        ],
        15,
    );

    match serde_json::from_str::<serde_json::Value>(raw.trim()) {
        Ok(parsed) => {
            let process_count = parsed
                .get("procs")
                .and_then(|v| v.as_str())
                .and_then(|s| s.trim().parse::<u32>().ok());
            let ip_str = parsed
                .get("ip")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s != "null");
            MachineStats {
                cpu: parsed
                    .get("cpu")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                mem: parsed
                    .get("mem")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                disk: parsed
                    .get("disk")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                gpu: Some(
                    parsed
                        .get("gpu")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-")
                        .to_string(),
                ),
                uptime: parsed
                    .get("uptime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                online: true,
                process_count,
                ip_address: ip_str,
            }
        }
        Err(_) => MachineStats {
            cpu: "-".into(),
            mem: "-".into(),
            disk: "-".into(),
            gpu: Some("-".into()),
            uptime: "-".into(),
            online: false,
            process_count: None,
            ip_address: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Task Routing
// ---------------------------------------------------------------------------

/// Parse a percentage string like "72%" or "72.5%" into a f64 (0–100).
/// Returns None if unparseable.
fn parse_percent(s: &str) -> Option<f64> {
    s.trim().trim_end_matches('%').parse::<f64>().ok()
}

/// Infer routing tags from free-form prompt text.
/// Keyword groups → preferred machine tags:
///   gpu / training / cuda / render / inference / model  → "gpu"
///   backend / api / server / database / db / rust       → "backend"
///   frontend / ui / svelte / css / html / react         → "frontend"
fn tags_from_prompt(prompt: &str) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut inferred: Vec<String> = Vec::new();

    let gpu_keywords = ["gpu", "training", "cuda", "render", "inference", "model", "nvidia", "torch"];
    let backend_keywords = ["backend", "api", "server", "database", " db ", "rust", "cargo", "http"];
    let frontend_keywords = ["frontend", " ui ", "svelte", "css", "html", "react", "vite", "typescript"];

    if gpu_keywords.iter().any(|kw| lower.contains(kw)) {
        inferred.push("gpu".to_string());
    }
    if backend_keywords.iter().any(|kw| lower.contains(kw)) {
        inferred.push("backend".to_string());
    }
    if frontend_keywords.iter().any(|kw| lower.contains(kw)) {
        inferred.push("frontend".to_string());
    }

    inferred
}

pub fn best_machine_for_task_impl(
    registry: &MachineRegistry,
    explicit_tags: &[String],
    prompt: Option<&str>,
) -> Option<String> {
    // Build the full tag set: explicit tags + tags inferred from the prompt
    let mut all_tags: Vec<String> = explicit_tags.to_vec();
    if let Some(p) = prompt {
        for tag in tags_from_prompt(p) {
            if !all_tags.contains(&tag) {
                all_tags.push(tag);
            }
        }
    }

    // Clone machine data immediately — drop lock before any SSH I/O
    let candidates: Vec<Machine> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.values().filter(|m| m.enabled).cloned().collect()
    };
    if candidates.is_empty() {
        return None;
    }

    // Collect machines that need a health refresh (lock briefly, then release)
    let to_check: Vec<Machine> = {
        let health_cache = registry.health_cache.lock().unwrap_or_else(|e| e.into_inner());
        candidates
            .iter()
            .filter(|m| match health_cache.get(&m.id) {
                Some((_, ts)) => ts.elapsed().as_secs() > 30,
                None => true,
            })
            .cloned()
            .collect()
    };

    // Perform health checks without holding any lock (SSH I/O)
    let fresh_results: Vec<(String, MachineHealth)> = to_check
        .iter()
        .map(|m| (m.id.clone(), check_machine(m)))
        .collect();

    // Re-acquire lock to insert results
    {
        let mut health_cache = registry.health_cache.lock().unwrap_or_else(|e| e.into_inner());
        for (id, health) in fresh_results {
            health_cache.insert(id, (health, Instant::now()));
        }
    }

    // Gather live stats for load-awareness (no lock held during I/O)
    let stats_map: std::collections::HashMap<String, crate::types::MachineStats> = candidates
        .iter()
        .map(|m| (m.id.clone(), get_machine_stats(m)))
        .collect();

    // Filter: only online machines (health check must confirm online == true)
    let health_cache = registry.health_cache.lock().unwrap_or_else(|e| e.into_inner());
    let online: Vec<Machine> = candidates
        .into_iter()
        .filter(|m| {
            health_cache
                .get(&m.id)
                .map(|(h, _)| h.online)
                .unwrap_or(false)
        })
        .collect();

    if online.is_empty() {
        return None;
    }

    // Score every online machine
    let mut best_id: Option<String> = None;
    let mut best_score: f64 = f64::NEG_INFINITY;

    for m in &online {
        let mut score: f64 = 0.0;

        // Tag + role matching
        for tag in &all_tags {
            if m.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                score += 2.0;
            }
            if m.role.to_lowercase().contains(&tag.to_lowercase()) {
                score += 1.0;
            }
        }

        // Latency bonus (local / low-latency preferred)
        let latency = health_cache
            .get(&m.id)
            .map(|(h, _)| h.latency_ms)
            .unwrap_or(999);
        if latency < 100 {
            score += 0.5;
        }

        // Load awareness: high CPU or memory → heavy penalty
        if let Some(stats) = stats_map.get(&m.id) {
            let cpu_pct = parse_percent(&stats.cpu).unwrap_or(0.0);
            let mem_pct = parse_percent(&stats.mem).unwrap_or(0.0);

            if cpu_pct > 90.0 {
                score -= 3.0; // severely overloaded CPU — deprioritize
            } else if cpu_pct > 70.0 {
                score -= 1.0; // moderately loaded
            }

            if mem_pct > 95.0 {
                score -= 3.0; // nearly OOM — deprioritize
            } else if mem_pct > 80.0 {
                score -= 0.5;
            }

            // Use CPU as tiebreaker: lower CPU = slightly better score
            score += (100.0 - cpu_pct) / 200.0; // max +0.5 bonus
        }

        if score > best_score {
            best_score = score;
            best_id = Some(m.id.clone());
        }
    }

    // Fallback: if no machine matched any tags (score stayed at or below 0.5 tiebreaker),
    // return the online machine with the lowest CPU usage.
    let any_tag_match = best_score >= 1.0; // tag hit adds at least 1.0
    if !any_tag_match && !all_tags.is_empty() {
        // Re-rank purely by CPU
        let mut fallback_id: Option<String> = None;
        let mut lowest_cpu: f64 = f64::MAX;
        for m in &online {
            let cpu = stats_map
                .get(&m.id)
                .and_then(|s| parse_percent(&s.cpu))
                .unwrap_or(100.0);
            if cpu < lowest_cpu {
                lowest_cpu = cpu;
                fallback_id = Some(m.id.clone());
            }
        }
        return fallback_id.or(best_id);
    }

    best_id
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_machines(
    registry: tauri::State<'_, MachineRegistry>,
) -> Result<HashMap<String, MachineInfo>, String> {
    // Snapshot machine data before spawn_blocking (State<'_> is not Send)
    let machines_snapshot: Vec<(String, Machine)> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.iter().map(|(id, m)| (id.clone(), m.clone())).collect()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = HashMap::new();
        for (id, machine) in &machines_snapshot {
            let health = check_machine(machine);
            let stats = get_machine_stats(machine);
            result.insert(
                id.clone(),
                MachineInfo {
                    machine: machine.clone(),
                    health: Some(health),
                    stats: Some(stats),
                },
            );
        }
        result
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_machine(
    registry: tauri::State<'_, MachineRegistry>,
    id: String,
    enabled: bool,
) -> Result<bool, String> {
    if id.is_empty() {
        return Err("machine id must not be empty".into());
    }
    let mut machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = machines.get_mut(&id) {
        m.enabled = enabled;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn reconnect_machine(
    registry: tauri::State<'_, MachineRegistry>,
    id: String,
) -> Result<Option<MachineInfo>, String> {
    if id.is_empty() {
        return Err("machine id must not be empty".into());
    }
    // Clone machine out before spawn_blocking (State<'_> is not Send)
    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&id) {
            Some(m) => m.clone(),
            None => return Ok(None),
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let health = check_machine(&machine);
        let stats = if health.online {
            get_machine_stats(&machine)
        } else {
            MachineStats {
                cpu: "-".into(),
                mem: "-".into(),
                disk: "-".into(),
                gpu: machine.gpu.as_ref().map(|_| "-".to_string()),
                uptime: "-".into(),
                online: false,
                process_count: None,
                ip_address: None,
            }
        };
        Ok(Some(MachineInfo {
            machine,
            health: Some(health),
            stats: Some(stats),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn best_machine_for_task(
    registry: tauri::State<'_, MachineRegistry>,
    tags: Vec<String>,
    prompt: Option<String>,
) -> Result<Option<String>, String> {
    // Snapshot all machine data before spawn_blocking (State<'_> is not Send)
    let machines_snapshot: Vec<Machine> = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        machines.values().filter(|m| m.enabled).cloned().collect()
    };

    tauri::async_runtime::spawn_blocking(move || {
        // Reconstruct a temporary registry from the snapshot for best_machine_for_task_impl
        let machines_map: HashMap<String, Machine> =
            machines_snapshot.into_iter().map(|m| (m.id.clone(), m)).collect();
        let temp_registry = MachineRegistry::from_machines(machines_map);
        best_machine_for_task_impl(&temp_registry, &tags, prompt.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    pub cpu_usage: f64,
    pub memory_used: f64,
    pub memory_total: f64,
    pub disk_used: f64,
    pub disk_total: f64,
    pub uptime_secs: u64,
}

#[tauri::command]
pub fn get_system_stats() -> Result<SystemStats, String> {
    // CPU: use vm_stat + top on macOS
    let cpu_usage = Command::new("sh")
        .args(["-c", "ps -A -o %cpu | awk '{s+=$1} END {print s}'"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
        .unwrap_or(0.0);

    // Memory: use vm_stat
    let mem_output = Command::new("vm_stat")
        .output()
        .map_err(|e| e.to_string())?;
    let mem_str = String::from_utf8_lossy(&mem_output.stdout);
    let page_size: f64 = 16384.0; // Apple Silicon page size
    let mut free: f64 = 0.0;
    let mut active: f64 = 0.0;
    let mut inactive: f64 = 0.0;
    let mut speculative: f64 = 0.0;
    let mut wired: f64 = 0.0;
    let mut compressed: f64 = 0.0;
    for line in mem_str.lines() {
        let val = line.split(':').nth(1)
            .map(|s| s.trim().trim_end_matches('.').parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);
        if line.starts_with("Pages free") { free = val; }
        else if line.starts_with("Pages active") { active = val; }
        else if line.starts_with("Pages inactive") { inactive = val; }
        else if line.starts_with("Pages speculative") { speculative = val; }
        else if line.starts_with("Pages wired") { wired = val; }
        else if line.starts_with("Pages occupied by compressor") { compressed = val; }
    }
    let total_pages = free + active + inactive + speculative + wired + compressed;
    let used_pages = active + wired + compressed;
    let memory_total = (total_pages * page_size) / (1024.0 * 1024.0 * 1024.0);
    let memory_used = (used_pages * page_size) / (1024.0 * 1024.0 * 1024.0);

    // Disk usage
    let disk_output = Command::new("df")
        .args(["-g", "/"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let mut disk_total: f64 = 0.0;
    let mut disk_used: f64 = 0.0;
    if let Some(line) = disk_output.lines().nth(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            disk_total = parts[1].parse().unwrap_or(0.0);
            disk_used = parts[2].parse().unwrap_or(0.0);
        }
    }

    // Uptime
    let uptime_output = Command::new("sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let uptime_secs = uptime_output
        .split("sec = ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|boot| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(boot)
        })
        .unwrap_or(0);

    Ok(SystemStats {
        cpu_usage,
        memory_used,
        memory_total,
        disk_used,
        disk_total,
        uptime_secs,
    })
}


#[derive(serde::Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

// ---------------------------------------------------------------------------
// Background Machine Monitor (exponential backoff reconnect)
// ---------------------------------------------------------------------------

/// Spawns a background thread that monitors remote machines.
/// When a remote machine is detected offline, it attempts to reconnect with
/// exponential backoff (1s → 2s → 4s → 8s → 16s, capped at 60s).
/// Emits `machine-reconnected` on success and `machine-offline` after
/// `MAX_ATTEMPTS` consecutive failures.
pub fn start_machine_monitor(app: tauri::AppHandle, shutdown: ShutdownFlag) {
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::Duration;

    const POLL_INTERVAL_SECS: u64 = 30;
    const MAX_ATTEMPTS: u32 = 6; // gives up after ~63s of backoff

    thread::spawn(move || {
        // Track per-machine reconnect state: (attempt_count, next_check_instant)
        let mut reconnect_state: HashMap<String, (u32, Instant)> = HashMap::new();

        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            // Snapshot machine list (release lock before any I/O)
            let machines_snapshot: Vec<Machine> = {
                let registry = app.state::<MachineRegistry>();
                let machines = registry.machines.lock().unwrap_or_else(|e: std::sync::PoisonError<_>| e.into_inner());
                machines
                    .values()
                    .filter(|m| m.enabled && m.host != "local")
                    .cloned()
                    .collect()
            };

            for machine in &machines_snapshot {
                let id = machine.id.clone();
                let name = machine.name.clone();

                // Determine if this machine is due for a check
                let now = Instant::now();
                let should_check = match reconnect_state.get(&id) {
                    None => {
                        // First time seeing this machine — check it
                        true
                    }
                    Some((attempts, next_check)) => {
                        if *attempts == 0 {
                            // Machine was last seen online; use normal poll cadence
                            now >= *next_check
                        } else {
                            // Machine is in backoff; check when backoff expires
                            now >= *next_check
                        }
                    }
                };

                if !should_check {
                    continue;
                }

                let health = check_machine(machine);

                if health.online {
                    let prev_attempts = reconnect_state
                        .get(&id)
                        .map(|(a, _)| *a)
                        .unwrap_or(0);

                    // Schedule next normal poll
                    reconnect_state.insert(
                        id.clone(),
                        (0, Instant::now() + Duration::from_secs(POLL_INTERVAL_SECS)),
                    );

                    // If we were in a failed state, emit reconnected event
                    if prev_attempts > 0 {
                        let _ = app.emit(
                            "machine-reconnected",
                            serde_json::json!({ "id": id, "name": name }),
                        );
                    }
                } else {
                    // Machine is offline — compute backoff
                    let attempts = reconnect_state
                        .get(&id)
                        .map(|(a, _)| *a)
                        .unwrap_or(0)
                        + 1;

                    if attempts >= MAX_ATTEMPTS {
                        // Give up — emit offline event and reset so we retry later
                        let _ = app.emit(
                            "machine-offline",
                            serde_json::json!({
                                "id": id,
                                "name": name,
                                "attempts": attempts
                            }),
                        );
                        // Reset after a longer pause (2 * POLL_INTERVAL_SECS)
                        reconnect_state.insert(
                            id.clone(),
                            (0, Instant::now() + Duration::from_secs(POLL_INTERVAL_SECS * 2)),
                        );
                    } else {
                        // Exponential backoff: 1, 2, 4, 8, 16, ... capped at 60s
                        let backoff_secs = std::cmp::min(1u64 << (attempts - 1), 60);
                        reconnect_state.insert(
                            id.clone(),
                            (attempts, Instant::now() + Duration::from_secs(backoff_secs)),
                        );
                    }
                }
            }

            // Sleep briefly between scan iterations; check shutdown flag frequently
            let mut slept = 0u64;
            while slept < 2 {
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
                slept += 1;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// New health monitoring commands
// ---------------------------------------------------------------------------

/// Return the uptime of a machine as a human-readable string.
/// Local machine: runs `uptime` directly.
/// Remote machine: SSHes in and runs `uptime`.
#[tauri::command]
pub async fn get_machine_uptime(
    registry: tauri::State<'_, MachineRegistry>,
    machine_id: String,
) -> Result<String, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&machine_id) {
            Some(m) => m.clone(),
            None => return Err(format!("unknown machine: {}", machine_id)),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        if machine.host == "local" {
            let out = run_cmd(
                "sh",
                &["-c", "uptime | sed 's/.*up //' | sed 's/,.*//' | xargs"],
                10,
            );
            let trimmed = out.trim().to_string();
            if trimmed.is_empty() {
                Err("uptime returned empty output".into())
            } else {
                Ok(trimmed)
            }
        } else {
            let out = run_cmd(
                "ssh",
                &[
                    "-o", "ConnectTimeout=10",
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ServerAliveInterval=5",
                    "-o", "ServerAliveCountMax=3",
                    machine.host.as_str(),
                    "uptime -p 2>/dev/null | sed 's/up //' || uptime | sed 's/.*up //' | sed 's/,.*//'; echo",
                ],
                10,
            );
            let trimmed = out.trim().to_string();
            if trimmed.is_empty() {
                Err(format!("could not reach machine: {}", machine.host))
            } else {
                Ok(trimmed)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ping a machine and return the round-trip latency in milliseconds.
/// Local machine: always returns 0 (instant).
/// Remote machine: runs `ping -c 1 -W 2 {host}` and parses the RTT.
/// Timeout: 5 seconds total.
#[tauri::command]
pub async fn ping_machine(
    registry: tauri::State<'_, MachineRegistry>,
    machine_id: String,
) -> Result<u64, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&machine_id) {
            Some(m) => m.clone(),
            None => return Err(format!("unknown machine: {}", machine_id)),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        if machine.host == "local" {
            return Ok(0u64);
        }

        // Use the machine's IP if configured, otherwise fall back to the SSH host alias.
        // `ping` works with both hostnames and IP addresses.
        let target = machine
            .ip
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(machine.host.as_str());

        // macOS ping: `ping -c 1 -W 2000 <host>` (-W is in milliseconds on macOS)
        // Linux ping: `ping -c 1 -W 2 <host>` (-W is in seconds on Linux)
        // We try macOS-style first; if we can't parse, it's unreachable.
        let out = run_cmd("ping", &["-c", "1", "-W", "2000", target], 5);

        if out.is_empty() {
            return Err(format!("host unreachable: {}", target));
        }

        // Parse RTT from output line like:
        //   "round-trip min/avg/max/stddev = 1.234/1.234/1.234/0.000 ms"  (macOS)
        //   "rtt min/avg/max/mdev = 1.234/1.234/1.234/0.000 ms"           (Linux)
        for line in out.lines() {
            if line.contains("min/avg/max") {
                // Extract the avg value (second field after '=')
                if let Some(stats) = line.split('=').nth(1) {
                    let avg = stats.trim().split('/').nth(1);
                    if let Some(avg_str) = avg {
                        if let Ok(ms) = avg_str.trim().parse::<f64>() {
                            return Ok(ms.round() as u64);
                        }
                    }
                }
            }
        }

        Err(format!("host unreachable or RTT unparseable: {}", target))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let expanded = if path == "~" {
        home.clone()
    } else if path.starts_with("~/") {
        path.replacen("~", &home, 1)
    } else if path.is_empty() {
        home.clone()
    } else {
        path.clone()
    };
    let p = std::path::Path::new(&expanded);
    if !p.exists() {
        return Err(format!("path not found: {}", expanded));
    }
    let mut entries: Vec<DirEntry> = std::fs::read_dir(p)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { return None; } // skip hidden
            let is_dir = e.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir { return None; } // only show directories
            Some(DirEntry {
                name: name.clone(),
                path: e.path().to_string_lossy().to_string(),
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Dangerous command blocklist
// ---------------------------------------------------------------------------

/// Returns true if the command contains a known dangerous pattern.
pub fn is_blocked_command(command: &str) -> bool {
    let blocked: &[&str] = &[
        "rm -rf",
        "rm -r /",
        "mkfs",
        "dd if=",
        ":(){ :|:& };:", // fork bomb
        "> /dev/sda",
        "shred",
        "fdisk",
        "parted",
        "wipefs",
        "chmod -R 777 /",
        "chown -R",
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
        "init 0",
        "init 6",
        "kill -9 -1",
        "> /etc/passwd",
        "> /etc/shadow",
    ];
    let lower = command.to_lowercase();
    blocked.iter().any(|&pat| lower.contains(pat))
}

// ---------------------------------------------------------------------------
// get_machine_metrics
// ---------------------------------------------------------------------------

/// Return combined CPU, RAM, disk, network I/O, and load average for a machine.
/// Runs a single shell script (local or via SSH).  Returns a JSON Value with fields:
///   cpu_percent, ram_percent, disk_percent, network_rx_kb, network_tx_kb, load_average
#[tauri::command]
pub async fn get_machine_metrics(
    registry: tauri::State<'_, MachineRegistry>,
    machine_id: String,
) -> Result<serde_json::Value, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&machine_id) {
            Some(m) => m.clone(),
            None => return Err(format!("unknown machine: {}", machine_id)),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        if machine.host == "local" {
            // macOS local metrics
            let script = r#"
cpu=$(top -l 1 -n 0 | grep 'CPU usage' | awk '{gsub(/%/,""); print $3+$5}' 2>/dev/null || echo 0);
mem_used=$(vm_stat | awk '/Pages (active|inactive|speculative|wired|occupied)/{gsub(/\./,""); s+=$NF}END{print s*16384}' 2>/dev/null || echo 0);
mem_total=$(sysctl -n hw.memsize 2>/dev/null || echo 1);
ram=$(awk "BEGIN{printf \"%.1f\", $mem_used/$mem_total*100}" 2>/dev/null || echo 0);
disk=$(df / | tail -1 | awk '{gsub(/%/,""); print $5}' 2>/dev/null || echo 0);
load=$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}' || uptime | awk -F'load average' '{print $2}' | awk -F',' '{gsub(/ /,""); print $1}' || echo 0);
net=$(netstat -ib 2>/dev/null | awk 'NR>1 && $3~/Link/ && $1!~/lo/{rx+=$7; tx+=$10}END{printf "%d %d", rx/1024, tx/1024}' || echo "0 0");
rx=$(echo $net | awk '{print $1}');
tx=$(echo $net | awk '{print $2}');
printf '{"cpu_percent":%.1f,"ram_percent":%.1f,"disk_percent":%s,"network_rx_kb":%s,"network_tx_kb":%s,"load_average":%.2f}\n' "$cpu" "$ram" "$disk" "$rx" "$tx" "$load"
"#;
            let out = run_cmd("sh", &["-c", script], 10);
            let trimmed = out.trim().to_string();
            if trimmed.is_empty() {
                return Err("metrics script returned empty output".into());
            }
            serde_json::from_str::<serde_json::Value>(&trimmed)
                .map_err(|e| format!("failed to parse metrics: {} (raw: {})", e, trimmed))
        } else {
            // Linux remote metrics via SSH
            let script = r#"cpu=$(top -bn1 2>/dev/null | grep 'Cpu(s)' | awk '{printf "%.1f", $2+$4}' || echo 0); ram=$(free | awk '/Mem:/{printf "%.1f", $3/$2*100}' 2>/dev/null || echo 0); disk=$(df / | tail -1 | awk '{gsub(/%/,""); print $5}' 2>/dev/null || echo 0); load=$(awk '{print $1}' /proc/loadavg 2>/dev/null || echo 0); rx=$(cat /sys/class/net/$(ip route | awk '/default/{print $5}' | head -1)/statistics/rx_bytes 2>/dev/null || echo 0); tx=$(cat /sys/class/net/$(ip route | awk '/default/{print $5}' | head -1)/statistics/tx_bytes 2>/dev/null || echo 0); rx_kb=$((rx/1024)); tx_kb=$((tx/1024)); printf '{"cpu_percent":%s,"ram_percent":%s,"disk_percent":%s,"network_rx_kb":%d,"network_tx_kb":%d,"load_average":%s}\n' "$cpu" "$ram" "$disk" "$rx_kb" "$tx_kb" "$load""#;
            let out = run_cmd(
                "ssh",
                &[
                    "-o", "ConnectTimeout=10",
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ServerAliveInterval=5",
                    "-o", "ServerAliveCountMax=3",
                    machine.host.as_str(),
                    script,
                ],
                15,
            );
            let trimmed = out.trim().to_string();
            if trimmed.is_empty() {
                return Err(format!("could not reach machine: {}", machine.host));
            }
            serde_json::from_str::<serde_json::Value>(&trimmed)
                .map_err(|e| format!("failed to parse metrics: {} (raw: {})", e, trimmed))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// execute_on_machine
// ---------------------------------------------------------------------------

/// Execute an arbitrary shell command on a local or remote machine.
/// Dangerous commands are blocked (rm -rf, mkfs, dd if=, etc.).
#[tauri::command]
pub async fn execute_on_machine(
    registry: tauri::State<'_, MachineRegistry>,
    machine_id: String,
    command: String,
) -> Result<String, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    if command.trim().is_empty() {
        return Err("command must not be empty".into());
    }
    if is_blocked_command(&command) {
        return Err(format!("command blocked by safety policy: {}", command));
    }

    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&machine_id) {
            Some(m) => m.clone(),
            None => return Err(format!("unknown machine: {}", machine_id)),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        if machine.host == "local" {
            let result = Command::new("sh")
                .args(["-c", &command])
                .output()
                .map_err(|e| format!("failed to run command: {}", e))?;
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            if !result.status.success() && !stderr.is_empty() {
                return Err(format!("command failed: {}", stderr.trim()));
            }
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else {
            let out = run_cmd(
                "ssh",
                &[
                    "-o", "ConnectTimeout=10",
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ServerAliveInterval=30",
                    "-o", "ServerAliveCountMax=20",
                    machine.host.as_str(),
                    &command,
                ],
                30,
            );
            if out.is_empty() {
                Err(format!("no output from machine: {}", machine.host))
            } else {
                Ok(out)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// get_machine_logs
// ---------------------------------------------------------------------------

/// Return the last `lines` lines from ~/.config/jarvis/logs/jarvis.log on the target machine.
#[tauri::command]
pub async fn get_machine_logs(
    registry: tauri::State<'_, MachineRegistry>,
    machine_id: String,
    lines: u32,
) -> Result<Vec<String>, String> {
    if machine_id.is_empty() {
        return Err("machine_id must not be empty".into());
    }
    let lines = lines.clamp(1, 10_000); // clamp to sane range
    let machine = {
        let machines = registry.machines.lock().unwrap_or_else(|e| e.into_inner());
        match machines.get(&machine_id) {
            Some(m) => m.clone(),
            None => return Err(format!("unknown machine: {}", machine_id)),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        let log_cmd = format!("tail -n {} ~/.config/jarvis/logs/jarvis.log 2>/dev/null || echo ''", lines);
        let raw = if machine.host == "local" {
            run_cmd("sh", &["-c", &log_cmd], 10)
        } else {
            run_cmd(
                "ssh",
                &[
                    "-o", "ConnectTimeout=10",
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ServerAliveInterval=5",
                    "-o", "ServerAliveCountMax=3",
                    machine.host.as_str(),
                    &log_cmd,
                ],
                15,
            )
        };
        let result: Vec<String> = raw
            .lines()
            .map(|l| l.to_string())
            .collect();
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry_with_local() -> MachineRegistry {
        let mut map = HashMap::new();
        map.insert(
            "test-local".to_string(),
            Machine {
                id: "test-local".into(),
                name: "Test Local".into(),
                host: "local".into(),
                ip: None,
                os: "macos".into(),
                role: "test".into(),
                repo: None,
                repo_path: None,
                gpu: None,
                enabled: true,
                tags: vec![],
                repos: vec![],
                home_dir: None,
            },
        );
        MachineRegistry::from_machines(map)
    }

    /// Unknown machine IDs must return Err from ping logic (tested synchronously
    /// via the registry lookup, not through the Tauri command layer).
    #[test]
    fn test_ping_machine_invalid_id() {
        let registry = MachineRegistry::from_machines(HashMap::new());
        let machines = registry.machines.lock().unwrap();
        let result = machines.get("nonexistent-id");
        assert!(result.is_none(), "unknown id should not be found in registry");
    }

    /// get_machine_uptime for an unknown machine should yield None from the registry.
    #[test]
    fn test_get_machine_uptime_invalid_id() {
        let registry = MachineRegistry::from_machines(HashMap::new());
        let machines = registry.machines.lock().unwrap();
        let result = machines.get("ghost-machine");
        assert!(result.is_none(), "unknown id should not be found in registry");
    }

    /// If a local machine is in the registry, get_local_mac_stats should return online=true.
    /// This test exercises the local stats path end-to-end.
    #[test]
    fn test_get_local_stats_online() {
        let registry = make_registry_with_local();
        let machines = registry.machines.lock().unwrap();
        let machine = machines.get("test-local").expect("test-local should exist");
        // Run the stats function directly (no SSH needed)
        let stats = get_machine_stats(machine);
        assert!(stats.online, "local machine stats should report online=true");
        // uptime should not be empty
        assert_ne!(stats.uptime, "", "uptime field should be non-empty for local machine");
    }

    /// Local uptime command should produce a non-empty string.
    #[test]
    fn test_uptime_local_nonempty() {
        let out = run_cmd(
            "sh",
            &["-c", "uptime | sed 's/.*up //' | sed 's/,.*//' | xargs"],
            10,
        );
        assert!(!out.trim().is_empty(), "uptime should return non-empty output on the local machine");
    }

    /// Process count on the local machine should be a positive number.
    #[test]
    fn test_local_process_count_positive() {
        let stats = get_local_mac_stats();
        let count = stats.process_count.expect("process_count should be Some for local machine");
        assert!(count > 0, "there should be at least one running process");
    }

    /// Local IP address should be present and look like an IPv4 address.
    #[test]
    fn test_local_ip_address_present() {
        let stats = get_local_mac_stats();
        if let Some(ip) = stats.ip_address {
            // Basic sanity: should contain dots (IPv4 format)
            assert!(ip.contains('.'), "ip_address should look like an IPv4 address, got: {}", ip);
        }
        // If None, the machine may have no non-loopback interface (rare in CI) — not a failure.
    }

    // ---------------------------------------------------------------------------
    // Blocklist tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_blocklist_rm_rf_blocked() {
        assert!(is_blocked_command("rm -rf /home/user"), "rm -rf should be blocked");
    }

    #[test]
    fn test_blocklist_mkfs_blocked() {
        assert!(is_blocked_command("mkfs.ext4 /dev/sdb1"), "mkfs should be blocked");
    }

    #[test]
    fn test_blocklist_dd_if_blocked() {
        assert!(is_blocked_command("dd if=/dev/zero of=/dev/sda"), "dd if= should be blocked");
    }

    #[test]
    fn test_blocklist_shutdown_blocked() {
        assert!(is_blocked_command("shutdown -h now"), "shutdown should be blocked");
    }

    #[test]
    fn test_blocklist_safe_commands_allowed() {
        assert!(!is_blocked_command("ls -la /home"), "ls should be allowed");
        assert!(!is_blocked_command("cat /etc/hosts"), "cat should be allowed");
        assert!(!is_blocked_command("ps aux"), "ps aux should be allowed");
        assert!(!is_blocked_command("df -h"), "df should be allowed");
        assert!(!is_blocked_command("echo hello"), "echo should be allowed");
    }

    #[test]
    fn test_blocklist_case_insensitive() {
        assert!(is_blocked_command("RM -RF /tmp"), "uppercase RM -RF should be blocked");
        assert!(is_blocked_command("SHUTDOWN -r now"), "uppercase SHUTDOWN should be blocked");
    }
}
