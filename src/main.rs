#![windows_subsystem = "windows"]

use taskpilot::app::TaskPilotApp;
use taskpilot::config::AppConfig;
use taskpilot::single_instance::SingleInstanceGuard;
use taskpilot::task_sources;
use taskpilot::tray::TrayManager;
use taskpilot::workspace::Workspace;

use eframe::egui;
use image::ImageReader;
use std::io::Cursor;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    // Ensure only one instance of TaskPilot is running.
    // If another instance exists, this signals it to restore its window and exits.
    let single_instance_guard = SingleInstanceGuard::acquire();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let start_minimized = args.contains(&"--minimized".to_string());

    // Parse --task-dir arguments (repeatable)
    let cli_task_dirs: Vec<std::path::PathBuf> = args
        .windows(2)
        .filter(|pair| pair[0] == "--task-dir")
        .map(|pair| std::path::PathBuf::from(&pair[1]))
        .collect();

    let workspace_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        })
        .join(".taskpilot");

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

    // Build the effective config with merged tasks
    let mut effective_config = config;
    effective_config.tasks = merged_tasks;

    let icon_png = include_bytes!("../assets/icon.png");
    let icon_img = ImageReader::new(Cursor::new(icon_png))
        .with_guessed_format()
        .expect("Failed to guess icon format")
        .decode()
        .expect("Failed to decode icon")
        .into_rgba8();
    let (w, h) = icon_img.dimensions();
    let window_icon = egui::IconData {
        rgba: icon_img.into_raw(),
        width: w,
        height: h,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 680.0])
            .with_title("TaskPilot")
            .with_icon(std::sync::Arc::new(window_icon))
            .with_visible(!start_minimized),
        ..Default::default()
    };

    eframe::run_native(
        "TaskPilot",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let tray = TrayManager::new(cc.egui_ctx.clone(), workspace.debug_log_path())
                .expect("Failed to create system tray");
            Ok(Box::new(TaskPilotApp::new(
                workspace,
                effective_config,
                tray,
                source_metadata,
                source_dirs,
                cli_task_dirs,
                single_instance_guard,
            )))
        }),
    )
}
