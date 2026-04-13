// Console-subsystem binary for CLI operations (--run, --list).
// Unlike the main GUI binary (windows_subsystem = "windows"), this
// binary has a normal console so stdout/stderr work in all terminals.

use taskpilot::config::{AppConfig, TriggerCondition};
use taskpilot::executor;
use taskpilot::logging::parse_log_level;
use taskpilot::task_sources;
use taskpilot::timezone;
use taskpilot::updater;
use taskpilot::workspace::{RunStatus, Workspace};

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse --run <task_name>
    // --version
    if args.contains(&"--version".to_string()) {
        println!("taskpilot-cli {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let run_task: Option<String> = args
        .windows(2)
        .find(|pair| pair[0] == "--run")
        .map(|pair| pair[1].clone());

    let list_tasks = args.contains(&"--list".to_string());
    let check_update = args.contains(&"--check-update".to_string());
    let do_update = args.contains(&"--update".to_string());

    if run_task.is_none() && !list_tasks && !check_update && !do_update {
        eprintln!("Usage: taskpilot-cli --list");
        eprintln!("       taskpilot-cli --run <task_name>");
        eprintln!("       taskpilot-cli --task-dir <path> --run <task_name>");
        eprintln!("       taskpilot-cli --check-update");
        eprintln!("       taskpilot-cli --update");
        eprintln!("       taskpilot-cli --version");
        std::process::exit(1);
    }

    // Parse --task-dir arguments (repeatable)
    let cli_task_dirs: Vec<std::path::PathBuf> = args
        .windows(2)
        .filter(|pair| pair[0] == "--task-dir")
        .map(|pair| std::path::PathBuf::from(&pair[1]))
        .collect();

    let workspace_dir = std::env::var("TASKPILOT_WORKSPACE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                })
                .join(".taskpilot")
        });

    let workspace = Arc::new(Workspace::new(workspace_dir));
    workspace.ensure_dirs().expect("Failed to create workspace");

    let config_path = workspace.config_path();
    let config = if config_path.exists() {
        AppConfig::load(&config_path).unwrap_or_else(|e| {
            eprintln!("Config error: {}. Using defaults.", e);
            AppConfig::default_config()
        })
    } else {
        let _ = AppConfig::save_default(&config_path);
        AppConfig::default_config()
    };

    workspace.set_log_level(parse_log_level(&config.general.log_level));

    // Merge config task_sources with CLI --task-dir arguments
    let mut source_dirs: Vec<std::path::PathBuf> = config
        .general
        .task_sources
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    for dir in &cli_task_dirs {
        if !source_dirs.contains(dir) {
            source_dirs.push(dir.clone());
        }
    }

    // Collect individual task config files
    let source_files: Vec<std::path::PathBuf> = config
        .general
        .task_configs
        .iter()
        .map(std::path::PathBuf::from)
        .collect();

    // Load and merge tasks from all sources
    let (merged_tasks, source_metadata) =
        task_sources::load_all(
            &config.tasks,
            &config_path,
            &source_dirs,
            &source_files,
            config.general.default_timezone.as_deref(),
            Some(&workspace),
        ).unwrap_or_else(|e| {
            eprintln!("Task source error: {}. Using local tasks only.", e);
            let mut map = std::collections::HashMap::new();
            for task in &config.tasks {
                map.insert(
                    task.name.clone(),
                    task_sources::TaskSourceInfo {
                        origin: task_sources::TaskOrigin::Local,
                        file_path: config_path.clone(),
                    },
                );
            }
            (config.tasks.clone(), map)
        });

    let mut effective_config = config;
    effective_config.tasks = merged_tasks;

    // ── --list ──────────────────────────────────────────────────────
    if list_tasks {
        println!("Available tasks:");
        for task in &effective_config.tasks {
            let origin = source_metadata
                .get(&task.name)
                .map(|info| match &info.origin {
                    task_sources::TaskOrigin::Local => "local".to_string(),
                    task_sources::TaskOrigin::External { dir } => {
                        format!("external: {}", dir.display())
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());
            let schedule = if let Some(cron) = &task.cron {
                let timezone_label = timezone::effective_timezone_label(
                    task,
                    effective_config.general.default_timezone.as_deref(),
                )
                .unwrap_or_else(|_| "system local time".to_string());
                format!("{} @ {}", cron, timezone_label)
            } else {
                "trigger-only".to_string()
            };
            println!("  {} [{}] -- {}", task.name, origin, schedule);
        }
        std::process::exit(0);
    }

    // ── --check-update ──────────────────────────────────────────────
    if check_update {
        println!("Current version: v{}", updater::CURRENT_VERSION);
        println!("Checking for updates...");
        match updater::check_for_update() {
            Ok(state) => {
                if let Some(ver) = &state.available_version {
                    println!("Update available: v{}", ver);
                    if let Some(url) = &state.release_url {
                        println!("Release page: {}", url);
                    }
                    println!("Run `taskpilot-cli --update` to download and apply.");
                } else {
                    println!("You are running the latest version.");
                }
            }
            Err(e) => {
                eprintln!("Failed to check for updates: {}", e);
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }

    // ── --update ────────────────────────────────────────────────────
    if do_update {
        println!("Current version: v{}", updater::CURRENT_VERSION);
        println!("Checking for updates...");
        match updater::check_for_update() {
            Ok(state) => {
                if let Some(ver) = &state.available_version {
                    println!("Update available: v{}", ver);
                    println!("Downloading...");
                    match updater::download_and_apply(&state) {
                        Ok(result) => {
                            println!("Update applied successfully!");
                            if result.gui_updated {
                                println!("  taskpilot.exe updated to v{}", result.version);
                            }
                            if result.cli_updated {
                                println!("  taskpilot-cli.exe updated to v{}", result.version);
                            }
                            if result.needs_restart {
                                println!("Restart TaskPilot to use the new version.");
                            }
                            // Save cleared update state
                            let state_path = updater::update_state_path(&workspace.root);
                            let mut new_state = state;
                            new_state.clear_update();
                            let _ = new_state.save(&state_path);
                        }
                        Err(e) => {
                            eprintln!("Failed to apply update: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    println!("You are running the latest version.");
                }
            }
            Err(e) => {
                eprintln!("Failed to check for updates: {}", e);
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }

    // ── --run <task_name> ───────────────────────────────────────────
    if let Some(task_name) = run_task {
        let task = match effective_config.tasks.iter().find(|t| t.name == task_name) {
            Some(t) => t.clone(),
            None => {
                eprintln!("Error: task '{}' not found.", task_name);
                eprintln!("Available tasks:");
                for t in &effective_config.tasks {
                    eprintln!("  {}", t.name);
                }
                std::process::exit(1);
            }
        };

        let exit_code = run_task_with_triggers(&task, &effective_config, &workspace, None);
        std::process::exit(exit_code);
    }
}

/// Run a task and recursively fire its triggers. Returns the exit code of the initial task.
fn run_task_with_triggers(
    task: &taskpilot::config::TaskConfig,
    config: &AppConfig,
    workspace: &Arc<Workspace>,
    triggered_by: Option<(&str, &RunStatus)>,
) -> i32 {
    if let Some((source, source_status)) = triggered_by {
        println!(
            "[taskpilot] Triggering '{}' (on completion of '{}', status: {:?})",
            task.name, source, source_status
        );
    } else {
        println!("Running task '{}'...", task.name);
    }

    let cancel = executor::new_cancel_token();
    let shell = executor::resolve_shell(task.shell, config.general.default_shell);
    let effective_timezone = timezone::effective_timezone_label(
        task,
        config.general.default_timezone.as_deref(),
    )
    .unwrap_or_else(|_| "system local time".to_string());
    let run = executor::execute_task_at(
        task,
        workspace,
        &cancel,
        shell,
        effective_timezone,
        chrono::Local::now(),
    );

    // Print output from the output.log file (new format) or legacy embedded fields
    if let Some(ref log_path) = run.output_log_path {
        if let Ok(content) = std::fs::read_to_string(log_path) {
            if !content.is_empty() {
                print!("{}", content);
            }
        }
    } else {
        if !run.stdout.is_empty() {
            print!("{}", run.stdout);
        }
        if !run.stderr.is_empty() {
            eprint!("{}", run.stderr);
        }
    }

    let exit_code = match run.status {
        RunStatus::Passed => {
            println!(
                "[taskpilot] Task '{}' passed ({}ms)",
                task.name,
                run.duration_ms.unwrap_or(0)
            );
            run.exit_code.unwrap_or(0)
        }
        RunStatus::Failed => {
            eprintln!(
                "[taskpilot] Task '{}' failed (exit code: {})",
                task.name,
                run.exit_code.unwrap_or(-1)
            );
            run.exit_code.unwrap_or(1)
        }
        RunStatus::Timeout => {
            eprintln!("[taskpilot] Task '{}' timed out", task.name);
            124
        }
        RunStatus::Running => {
            eprintln!("[taskpilot] Task '{}' in unexpected state", task.name);
            1
        }
        RunStatus::Stopped => {
            eprintln!("[taskpilot] Task '{}' was stopped", task.name);
            130
        }
    };

    // Fire matching triggers
    for trigger_cfg in &task.triggers {
        let should_fire = match trigger_cfg.condition {
            TriggerCondition::Success => run.status == RunStatus::Passed,
            TriggerCondition::Failure => run.status == RunStatus::Failed || run.status == RunStatus::Timeout,
            TriggerCondition::Always => run.status != RunStatus::Running,
        };
        if should_fire {
            if let Some(target) = config.tasks.iter().find(|t| t.name == trigger_cfg.task) {
                run_task_with_triggers(target, config, workspace, Some((&task.name, &run.status)));
            } else {
                eprintln!(
                    "[taskpilot] Warning: trigger target '{}' not found",
                    trigger_cfg.task
                );
            }
        }
    }

    exit_code
}
