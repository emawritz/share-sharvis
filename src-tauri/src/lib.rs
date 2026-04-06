mod app_logs;
mod types;
mod conflicts;
mod backup;
mod machines;
mod tasks;
mod jsonl;
mod session;
mod pipelines;
mod planning;
mod config;
mod github;
mod snapshots;
mod visibility;
mod webhooks;
mod messages;
mod rules;
mod notifications;
mod task_history;
mod capabilities;
mod crons;
mod whatsapp;
mod db;
mod knowledge;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

/// Shared flag to signal background threads to stop on app shutdown.
pub type ShutdownFlag = Arc<AtomicBool>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        // updater plugin removed — requires pubkey config; add back when update infra is ready
        .manage(app_logs::AppLogs::new())
        .manage(machines::MachineRegistry::new())
        .manage(pipelines::PipelineStore::new())
        .manage(planning::PlanningStore::new())
        .manage(Arc::new(AtomicBool::new(false)) as ShutdownFlag)
        .setup(|app| {
            // Initialize TaskStore with app data dir for persistence
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
            let state_file = app_data_dir
                .join("jarvis-state.json")
                .to_string_lossy()
                .to_string();
            app.manage(tasks::TaskStore::new(state_file));

            // Initialize SQLite database and run JSON → SQLite migrations
            let database = db::Db::open().expect("Failed to open SQLite database");
            db::migrate_from_json(&database);
            app.manage(database);

            // Auto-migrate: generate TOML config with existing machines if no config exists
            if !config::config_exists() {
                let default_cfg = config::JarvisConfig {
                    session: config::SessionConfig::default(),
                    machines: vec![
                        config::MachineConfig {
                            id: "main".into(),
                            name: "MAIN".into(),
                            host: "local".into(),
                            ip: None,
                            os: "macos".into(),
                            role: "orchestrator/backend".into(),
                            gpu: None,
                            enabled: true,
                            tags: vec!["backend".into(), "orchestrator".into(), "macos".into(), "local".into()],
                            repos: vec![config::RepoConfigToml {
                                name: "my-project".into(),
                                path: "~/projects/my-project".into(),
                                github: "user/my-project".into(),
                            }],
                            home_dir: None,
                        },
                        config::MachineConfig {
                            id: "worker".into(),
                            name: "WORKER".into(),
                            host: "worker".into(),
                            ip: None,
                            os: "linux".into(),
                            role: "frontend + GPU".into(),
                            gpu: Some("RTX 3070".into()),
                            enabled: true,
                            tags: vec!["frontend".into(), "gpu".into(), "linux".into(), "remote".into()],
                            repos: vec![config::RepoConfigToml {
                                name: "my-frontend".into(),
                                path: "~/projects/my-frontend".into(),
                                github: "user/my-frontend".into(),
                            }],
                            home_dir: Some("/home/user".into()),
                        },
                        config::MachineConfig {
                            id: "nomad".into(),
                            name: "NOMAD".into(),
                            host: "nomad".into(),
                            ip: None,
                            os: "linux".into(),
                            role: "flexible/backup".into(),
                            gpu: None,
                            enabled: false,
                            tags: vec!["backup".into(), "flexible".into(), "remote".into()],
                            repos: Vec::new(),
                            home_dir: None,
                        },
                    ],
                };
                let _ = config::save_config(&default_cfg);
            }

            // Setup system tray
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("JARVIS - Mission Control")
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    if id == "show" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    } else if id == "hide" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    } else if id == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Start background session monitor with shutdown flag
            let handle = app.handle().clone();
            let shutdown = app.state::<ShutdownFlag>().inner().clone();
            session::start_monitor(handle, shutdown);

            // Start background machine reconnect monitor (exponential backoff)
            {
                let monitor_handle = app.handle().clone();
                let monitor_shutdown = app.state::<ShutdownFlag>().inner().clone();
                machines::start_machine_monitor(monitor_handle, monitor_shutdown);
            }

            // Start background cron checker (every 60s)
            {
                use std::thread;
                use std::time::Duration;
                use std::sync::atomic::Ordering;
                let cron_handle = app.handle().clone();
                let cron_shutdown = app.state::<ShutdownFlag>().inner().clone();
                thread::spawn(move || {
                    loop {
                        thread::sleep(Duration::from_secs(60));
                        if cron_shutdown.load(Ordering::Relaxed) { break; }
                        let now = chrono::Utc::now();
                        let fired = crons::check_and_fire(now, &cron_handle);
                        for (_id, target, prompt) in fired {
                            let store = cron_handle.state::<tasks::TaskStore>();
                            tasks::send_task_internal(&cron_handle, &store, &target, &prompt, false);
                        }
                    }
                });
            }

            // Start WhatsApp bridge HTTP server on :3141
            {
                let wa_handle = app.handle().clone();
                let wa_shutdown = app.state::<ShutdownFlag>().inner().clone();
                whatsapp::start_server(wa_handle, wa_shutdown);
            }

            // Start PIXEL dashboard (status_server.py on port 7777) with retry
            {
                let dash_shutdown = app.state::<ShutdownFlag>().inner().clone();
                let registry = app.state::<machines::MachineRegistry>();
                let (pixel_host, pixel_home) = {
                    let machines = registry.machines.lock().unwrap();
                    let m = machines.get("pixel");
                    let host = m.map(|m| m.host.clone()).unwrap_or_else(|| "pixel".into());
                    let home = m.and_then(|m| m.home_dir.clone()).unwrap_or_else(|| "/home/user".into());
                    (host, home)
                    // drop(machines) — lock released here
                };
                std::thread::spawn(move || {
                    const MAX_RETRIES: u32 = 3;
                    // Give machines a moment to initialize
                    std::thread::sleep(std::time::Duration::from_secs(3));

                    for attempt in 1..=MAX_RETRIES {
                        if dash_shutdown.load(std::sync::atomic::Ordering::Relaxed) { return; }

                        let cmd = format!(
                            "kill $(lsof -ti:7777) 2>/dev/null; sleep 1; nohup python3 {home}/pixel-dashboard/status_server.py &>{home}/pixel-dashboard/server.log & disown; sleep 2; DISPLAY=:1 xdg-open http://localhost:7777 &>/dev/null & echo PIXEL_DASHBOARD_STARTED",
                            home = pixel_home,
                        );

                        // Quick SSH check before full command
                        let ping = std::process::Command::new("ssh")
                            .args(["-o", "ConnectTimeout=3", "-o", "BatchMode=yes", pixel_host.as_str(), "echo ok"])
                            .output();
                        if ping.is_err() || !ping.unwrap().status.success() {
                            log::warn!("PIXEL dashboard: SSH to {} unreachable, skipping", pixel_host);
                            break;
                        }

                        log::info!("PIXEL dashboard: attempt {}/{}", attempt, MAX_RETRIES);

                        let child = std::process::Command::new("ssh")
                            .args([
                                "-o", "ConnectTimeout=5",
                                "-o", "BatchMode=yes",
                                "-o", "ServerAliveInterval=30",
                                "-o", "ServerAliveCountMax=20",
                                pixel_host.as_str(),
                                cmd.as_str(),
                            ])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();

                        let mut success = false;
                        match child {
                            Ok(mut child) => {
                                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                                loop {
                                    match child.try_wait() {
                                        Ok(Some(status)) => {
                                            if status.success() {
                                                log::info!("PIXEL dashboard started on http://{}:7777", pixel_host);
                                                success = true;
                                            } else {
                                                log::warn!("PIXEL dashboard SSH exited with {}", status);
                                            }
                                            break;
                                        }
                                        Ok(None) => {
                                            if dash_shutdown.load(std::sync::atomic::Ordering::Relaxed)
                                                || std::time::Instant::now() >= deadline
                                            {
                                                let _ = child.kill();
                                                let _ = child.wait();
                                                log::warn!("PIXEL dashboard SSH killed (shutdown or timeout)");
                                                break;
                                            }
                                            std::thread::sleep(std::time::Duration::from_millis(500));
                                        }
                                        Err(e) => {
                                            log::warn!("PIXEL dashboard: error polling SSH process: {}", e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("PIXEL dashboard: failed to spawn SSH: {}", e);
                            }
                        }

                        if success { break; }
                        if attempt < MAX_RETRIES {
                            log::info!("PIXEL dashboard: retrying in 5s...");
                            std::thread::sleep(std::time::Duration::from_secs(5));
                        } else {
                            log::warn!("PIXEL dashboard: all {} attempts failed", MAX_RETRIES);
                        }
                    }
                });
            }

            // Start wa-bridge (WhatsApp ↔ JARVIS bridge) with restart-on-crash
            {
                let wa_bridge_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .map(|p| p.join("wa-bridge"));

                if let Some(ref dir) = wa_bridge_dir {
                    if dir.join("index.js").exists() {
                        let wa_bridge_shutdown = app.state::<ShutdownFlag>().inner().clone();
                        let dir = dir.clone();
                        std::thread::spawn(move || {
                            const MAX_RETRIES: u32 = 5;

                            // Create log directory and open log file for stderr
                            let log_dir = dirs::home_dir()
                                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                                .join(".config/jarvis/logs");
                            let _ = std::fs::create_dir_all(&log_dir);
                            let log_path = log_dir.join("wa-bridge.log");

                            std::thread::sleep(std::time::Duration::from_secs(2));

                            for attempt in 1..=MAX_RETRIES {
                                if wa_bridge_shutdown.load(std::sync::atomic::Ordering::Relaxed) { return; }

                                let stderr_file = std::fs::File::create(&log_path);
                                let stderr_stdio = match stderr_file {
                                    Ok(f) => std::process::Stdio::from(f),
                                    Err(e) => {
                                        log::warn!("wa-bridge: could not open log file {:?}: {}", log_path, e);
                                        std::process::Stdio::null()
                                    }
                                };

                                // Check if port 3142 is already in use (wa-bridge already running externally)
                                if std::net::TcpStream::connect("127.0.0.1:3142").is_ok() {
                                    log::info!("wa-bridge: port 3142 already in use, skipping (running externally)");
                                    return;
                                }

                                log::info!("wa-bridge: starting (attempt {}/{})", attempt, MAX_RETRIES);

                                let child = std::process::Command::new("node")
                                    .arg("index.js")
                                    .current_dir(&dir)
                                    .stdout(std::process::Stdio::null())
                                    .stderr(stderr_stdio)
                                    .spawn();

                                let mut shutdown_requested = false;
                                match child {
                                    Ok(mut child) => {
                                        log::info!("wa-bridge started (pid={})", child.id());
                                        // Keep alive until shutdown or crash
                                        loop {
                                            match child.try_wait() {
                                                Ok(Some(status)) => {
                                                    log::warn!("wa-bridge exited with {}", status);
                                                    break;
                                                }
                                                Ok(None) => {
                                                    if wa_bridge_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                                                        let _ = child.kill();
                                                        let _ = child.wait();
                                                        log::info!("wa-bridge killed on shutdown");
                                                        shutdown_requested = true;
                                                        break;
                                                    }
                                                    std::thread::sleep(std::time::Duration::from_secs(2));
                                                }
                                                Err(e) => {
                                                    log::warn!("wa-bridge poll error: {}", e);
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => log::warn!("wa-bridge failed to start: {}", e),
                                }

                                if shutdown_requested { return; }
                                if attempt < MAX_RETRIES {
                                    log::info!("wa-bridge: restarting in 5s (attempt {}/{})", attempt + 1, MAX_RETRIES);
                                    std::thread::sleep(std::time::Duration::from_secs(5));
                                } else {
                                    log::warn!("wa-bridge: all {} restart attempts exhausted", MAX_RETRIES);
                                }
                            }
                        });
                    } else {
                        log::info!("wa-bridge directory not found at {:?}, skipping", dir);
                    }
                }
            }

            // Start livekit-agent (voice AI agent) with restart-on-crash
            {
                let livekit_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .map(|p| p.join("livekit-agent"));

                if let Some(ref dir) = livekit_dir {
                    if dir.join("agent.py").exists() {
                        let livekit_shutdown = app.state::<ShutdownFlag>().inner().clone();
                        let dir = dir.clone();
                        std::thread::spawn(move || {
                            const MAX_RETRIES: u32 = 5;

                            let log_dir = dirs::home_dir()
                                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                                .join(".config/jarvis/logs");
                            let _ = std::fs::create_dir_all(&log_dir);
                            let log_path = log_dir.join("livekit-agent.log");

                            std::thread::sleep(std::time::Duration::from_secs(2));

                            for attempt in 1..=MAX_RETRIES {
                                if livekit_shutdown.load(std::sync::atomic::Ordering::Relaxed) { return; }

                                let stderr_file = std::fs::File::create(&log_path);
                                let stderr_stdio = match stderr_file {
                                    Ok(f) => std::process::Stdio::from(f),
                                    Err(e) => {
                                        log::warn!("livekit-agent: could not open log file {:?}: {}", log_path, e);
                                        std::process::Stdio::null()
                                    }
                                };

                                // Check if port 3144 is already in use (agent already running)
                                if std::net::TcpStream::connect("127.0.0.1:3144").is_ok() {
                                    log::info!("livekit-agent: port 3144 already in use, skipping");
                                    return;
                                }

                                log::info!("livekit-agent: starting (attempt {}/{})", attempt, MAX_RETRIES);

                                let venv_python = dir.join(".venv/bin/python");
                                let child = std::process::Command::new(&venv_python)
                                    .args(["agent.py", "dev"])
                                    .current_dir(&dir)
                                    .stdout(std::process::Stdio::null())
                                    .stderr(stderr_stdio)
                                    .spawn();

                                let mut shutdown_requested = false;
                                match child {
                                    Ok(mut child) => {
                                        log::info!("livekit-agent started (pid={})", child.id());
                                        loop {
                                            match child.try_wait() {
                                                Ok(Some(status)) => {
                                                    log::warn!("livekit-agent exited with {}", status);
                                                    break;
                                                }
                                                Ok(None) => {
                                                    if livekit_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                                                        let _ = child.kill();
                                                        let _ = child.wait();
                                                        log::info!("livekit-agent killed on shutdown");
                                                        shutdown_requested = true;
                                                        break;
                                                    }
                                                    std::thread::sleep(std::time::Duration::from_secs(2));
                                                }
                                                Err(e) => {
                                                    log::warn!("livekit-agent poll error: {}", e);
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => log::warn!("livekit-agent failed to start: {}", e),
                                }

                                if shutdown_requested { return; }
                                if attempt < MAX_RETRIES {
                                    log::info!("livekit-agent: restarting in 5s (attempt {}/{})", attempt + 1, MAX_RETRIES);
                                    std::thread::sleep(std::time::Duration::from_secs(5));
                                } else {
                                    log::warn!("livekit-agent: all {} restart attempts exhausted", MAX_RETRIES);
                                }
                            }
                        });
                    } else {
                        log::info!("livekit-agent/agent.py not found at {:?}, skipping", dir);
                    }
                }
            }

            {
                use tauri_plugin_log::{Target, TargetKind, RotationStrategy};

                let log_dir = dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join(".config/jarvis/logs");

                let level = if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Warn
                };

                let mut targets = vec![
                    Target::new(TargetKind::Folder {
                        path: log_dir,
                        file_name: Some("jarvis".into()),
                    }),
                ];
                if cfg!(debug_assertions) {
                    targets.push(Target::new(TargetKind::Stdout));
                }

                app.handle().plugin(
                    tauri_plugin_log::Builder::new()
                        .targets(targets)
                        .level(level)
                        .max_file_size(5_000_000) // 5 MB
                        .rotation_strategy(RotationStrategy::KeepSome(3))
                        .build(),
                )?;
                log::info!("JARVIS logging initialized (level={:?})", level);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    let flag = window.app_handle().state::<ShutdownFlag>();
                    flag.store(true, Ordering::Relaxed);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // machines
            machines::get_machines,
            machines::toggle_machine,
            machines::reconnect_machine,
            machines::best_machine_for_task,
            machines::get_machine_uptime,
            machines::ping_machine,
            machines::get_machine_metrics,
            machines::execute_on_machine,
            machines::get_machine_logs,
            // tasks
            tasks::send_task,
            tasks::get_tasks,
            tasks::execute_action,
            tasks::get_config,
            tasks::set_config,
            tasks::send_task_chain,
            tasks::send_task_graph,
            // session
            session::get_session_data,
            session::get_atlas_activity,
            session::get_pixel_activity_cmd,
            session::get_agent_details,
            session::get_agent_log,
            session::get_session_summary,
            session::get_recent_prompts,
            session::get_activity_heatmap,
            // pipelines
            pipelines::get_pipelines,
            pipelines::run_pipeline,
            pipelines::stop_pipeline,
            // github
            github::list_prs,
            github::create_pr,
            github::merge_pr,
            github::get_checks,
            github::compare_branches,
            github::get_git_diff,
            github::get_pr_comments,
            github::add_pr_comment,
            github::get_pr_files,
            // planning
            planning::start_planning,
            planning::get_planning_state,
            planning::approve_plan,
            planning::add_planning_feedback,
            planning::cancel_planning,
            planning::retry_failed_steps,
            planning::get_repo_statuses,
            planning::get_repo_branches,
            planning::switch_branch,
            planning::get_planning_history,
            planning::clear_planning_history,
            planning::export_planning_session,
            planning::get_planning_metrics,
            planning::duplicate_planning_session,
            // visibility
            visibility::get_timeline,
            visibility::get_token_stats,
            visibility::get_daily_stats,
            visibility::get_top_tools,
            // config
            config::get_jarvis_config,
            config::save_jarvis_config,
            config::is_first_launch,
            config::get_detected_local,
            config::test_ssh_connection,
            config::check_machine_connections,
            config::run_fix_command,
            config::get_budget_limit,
            config::set_budget_limit,
            // snapshots
            snapshots::save_session_snapshot,
            snapshots::list_session_snapshots,
            snapshots::restore_session_snapshot,
            snapshots::delete_session_snapshot,
            snapshots::search_snapshots,
            snapshots::tag_snapshot,
            // workspaces
            snapshots::list_workspaces,
            snapshots::save_workspace,
            snapshots::switch_workspace,
            // webhooks
            webhooks::get_webhook_config,
            webhooks::save_webhook_settings,
            webhooks::test_webhook,
            webhooks::get_webhook_deliveries,
            // messages
            messages::send_agent_message,
            messages::get_agent_messages,
            messages::mark_messages_read,
            messages::clear_messages,
            messages::save_team_memory,
            messages::get_team_memories,
            messages::delete_team_memory,
            messages::get_team_context,
            messages::search_team_memories,
            messages::get_memory_categories,
            messages::pin_memory,
            // notifications
            notifications::get_notifications_enabled,
            notifications::set_notifications_enabled,
            notifications::get_notification_prefs,
            notifications::save_notification_prefs,
            notifications::get_notification_history,
            // task_history
            task_history::get_task_history,
            task_history::count_task_history,
            task_history::clear_task_history,
            task_history::search_task_history,
            task_history::get_task_history_by_machine,
            // rules
            rules::get_rules,
            rules::save_rule,
            rules::delete_rule,
            rules::toggle_rule,
            rules::get_rule_history,
            rules::reorder_rules,
            rules::dry_run_rule,
            // capabilities
            capabilities::get_machine_capabilities,
            capabilities::get_single_machine_capabilities,
            // system stats
            machines::get_system_stats,
            // machine metrics / quick command / logs
            machines::get_machine_metrics,
            machines::execute_on_machine,
            machines::get_machine_logs,
            // filesystem
            machines::list_dir,
            // crons
            crons::get_crons,
            crons::save_cron,
            crons::delete_cron,
            crons::toggle_cron,
            crons::get_cron_next_runs,
            crons::validate_cron_expr_cmd,
            // conflicts
            conflicts::detect_conflicts,
            conflicts::resolve_conflict,
            conflicts::auto_resolve_conflicts,
            conflicts::get_conflict_diff,
            // app logs
            app_logs::get_app_logs,
            app_logs::clear_app_logs,
            app_logs::get_app_log_stats,
            // backup
            backup::export_config,
            backup::import_config,
            backup::list_backups,
            // whatsapp
            whatsapp::get_whatsapp_status,
            // knowledge
            knowledge::save_knowledge,
            knowledge::get_knowledge,
            knowledge::delete_knowledge,
            knowledge::star_knowledge,
            knowledge::search_knowledge,
            // database admin
            db::get_db_stats,
            db::vacuum_db,
            db::get_db_migration_version,
            // system / diagnostics
            types::get_app_version,
            types::get_environment_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
