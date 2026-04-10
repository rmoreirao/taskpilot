#![windows_subsystem = "windows"]

use taskpilot::app::TaskPilotApp;
use taskpilot::config::AppConfig;
use taskpilot::logging::parse_log_level;
use taskpilot::renderer;
use taskpilot::single_instance::SingleInstanceGuard;
use taskpilot::task_sources;
use taskpilot::tray::TrayManager;
use taskpilot::workspace::{self, Workspace};

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

    if args.contains(&"--version".to_string()) {
        println!("taskpilot {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

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

    workspace.set_log_level(parse_log_level(&config.general.log_level));

    // Install a panic hook that writes to the debug log so crashes are visible
    // even with #![windows_subsystem = "windows"] (no console attached).
    let panic_log_path = workspace.debug_log_path();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{info}");
        let _ = workspace::append_debug_log(&panic_log_path, "PANIC", &msg);
    }));

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
        task_sources::load_all(&config.tasks, &config_path, &source_dirs, &source_files, Some(&workspace)).unwrap_or_else(|e| {
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
    let window_icon = Arc::new(egui::IconData {
        rgba: icon_img.into_raw(),
        width: w,
        height: h,
    });

    // --- Renderer selection ---------------------------------------------------
    let pref = renderer::parse_renderer_arg(&args);
    let primary = renderer::select_renderer(pref);
    let _ = workspace.append_debug_log(
        "renderer",
        &format!(
            "preference={pref:?}, selected={}",
            renderer::renderer_name(primary)
        ),
    );

    let debug_log = workspace.debug_log_path();
    let _ = workspace.append_debug_log("main", "Starting eframe GUI...");

    // Wrap the single-instance guard so it can survive a renderer retry.  The
    // first app-creator closure that actually executes `.take()`s it; if the
    // primary renderer fails *before* the closure runs (adapter / OpenGL errors)
    // the guard stays available for the fallback attempt.
    let guard_slot: Arc<std::sync::Mutex<Option<SingleInstanceGuard>>> =
        Arc::new(std::sync::Mutex::new(Some(single_instance_guard)));

    // --- Primary attempt ------------------------------------------------------
    let result = {
        let options = eframe::NativeOptions {
            renderer: primary,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1024.0, 680.0])
                .with_title("TaskPilot")
                .with_icon(window_icon.clone())
                .with_visible(!start_minimized),
            ..Default::default()
        };

        let ws = workspace.clone();
        let cfg = effective_config.clone();
        let meta = source_metadata.clone();
        let dirs = source_dirs.clone();
        let files = source_files.clone();
        let cli = cli_task_dirs.clone();
        let guard = guard_slot.clone();

        eframe::run_native(
            "TaskPilot",
            options,
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
                let tray = TrayManager::new(cc.egui_ctx.clone(), ws.debug_log_path())
                    .expect("Failed to create system tray");
                let sig = guard
                    .lock()
                    .expect("guard lock poisoned")
                    .take()
                    .expect("single-instance guard already consumed");
                Ok(Box::new(TaskPilotApp::new(
                    ws, cfg, tray, meta, dirs, files, cli, sig,
                )))
            }),
        )
    };

    if result.is_ok() {
        return result;
    }

    // --- Fallback attempt -----------------------------------------------------
    let primary_err = result.unwrap_err();
    let fallback = renderer::alternate_renderer(primary);
    let _ = workspace::append_debug_log(
        &debug_log,
        "renderer",
        &format!(
            "{} failed ({}), retrying with {}",
            renderer::renderer_name(primary),
            primary_err,
            renderer::renderer_name(fallback),
        ),
    );

    let fallback_options = eframe::NativeOptions {
        renderer: fallback,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 680.0])
            .with_title("TaskPilot")
            .with_icon(window_icon)
            .with_visible(!start_minimized),
        ..Default::default()
    };

    let ws = workspace.clone();
    let guard = guard_slot.clone();

    let fallback_result = eframe::run_native(
        "TaskPilot",
        fallback_options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let tray = TrayManager::new(cc.egui_ctx.clone(), ws.debug_log_path())
                .expect("Failed to create system tray");
            let sig = guard
                .lock()
                .expect("guard lock poisoned")
                .take()
                .expect("single-instance guard already consumed");
            Ok(Box::new(TaskPilotApp::new(
                ws,
                effective_config,
                tray,
                source_metadata,
                source_dirs,
                source_files,
                cli_task_dirs,
                sig,
            )))
        }),
    );

    if let Err(ref e) = fallback_result {
        let msg = format!(
            "Both renderers failed.\n  {}: {}\n  {}: {}\n\
             Suggestions:\n  \
             - Install GPU drivers for your display adapter\n  \
             - Use Remote Desktop with GPU redirection enabled\n  \
             - Place a Mesa3D opengl32.dll next to taskpilot.exe for software OpenGL",
            renderer::renderer_name(primary),
            primary_err,
            renderer::renderer_name(fallback),
            e,
        );
        let _ = workspace::append_debug_log(&debug_log, "FATAL", &msg);
    }
    fallback_result
}
