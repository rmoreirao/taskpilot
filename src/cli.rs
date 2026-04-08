// Console-subsystem binary for CLI operations (--run, --list).
// Unlike the main GUI binary (windows_subsystem = "windows"), this
// binary has a normal console so stdout/stderr work in all terminals.

use taskpilot::config::AppConfig;
use taskpilot::executor;
use taskpilot::task_sources;
use taskpilot::workspace::{RunStatus, Workspace};

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse --run <task_name>
    let run_task: Option<String> = args
        .windows(2)
        .find(|pair| pair[0] == "--run")
        .map(|pair| pair[1].clone());

    let list_tasks = args.contains(&"--list".to_string());

    if run_task.is_none() && !list_tasks {
        eprintln!("Usage: taskpilot-cli --list");
        eprintln!("       taskpilot-cli --run <task_name>");
        eprintln!("       taskpilot-cli --task-dir <path> --run <task_name>");
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

    // Load and merge tasks from all sources
    let (merged_tasks, source_metadata) =
        task_sources::load_all(&config.tasks, &config_path, &source_dirs).unwrap_or_else(|e| {
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
            println!("  {} [{}] -- {}", task.name, origin, task.cron);
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

        println!("Running task '{}'...", task.name);
        let run = executor::execute_task(&task, &workspace);

        if !run.stdout.is_empty() {
            print!("{}", run.stdout);
        }
        if !run.stderr.is_empty() {
            eprint!("{}", run.stderr);
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
        };

        std::process::exit(exit_code);
    }
}
