mod app;
mod autostart;
mod config;
mod executor;
mod scheduler;
mod tray;
mod ui;
mod workspace;

use app::TaskPilotApp;
use config::AppConfig;
use eframe::egui;
use image::ImageReader;
use std::io::Cursor;
use std::sync::Arc;
use tray::TrayManager;
use workspace::Workspace;

fn main() -> eframe::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let start_minimized = args.contains(&"--minimized".to_string());

    let workspace_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
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
            let tray = TrayManager::new(cc.egui_ctx.clone())
                .expect("Failed to create system tray");
            Ok(Box::new(TaskPilotApp::new(workspace, config, tray)))
        }),
    )
}
