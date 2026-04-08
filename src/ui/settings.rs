use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::{TaskPilotApp, UpdateProgress};
use crate::autostart;
use crate::workspace::RunStatus;
use eframe::egui;

pub fn render_settings(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Configuration file and preferences").color(MUTED));
    ui.add_space(12.0);

    // Windows Auto-start section
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("🚀 Startup Options").strong());
    ui.add_space(8.0);

    let mut start_with_windows = app.config.general.start_with_windows;
    let checkbox_response = ui.checkbox(&mut start_with_windows, "Start with Windows");
    
    if checkbox_response.changed() {
        app.config.general.start_with_windows = start_with_windows;
        
        // Apply changes to registry
        let result = if start_with_windows {
            autostart::enable_autostart()
        } else {
            autostart::disable_autostart()
        };

        match result {
            Ok(_) => {
                // Save config
                if let Ok(content) = toml::to_string_pretty(&app.config) {
                    let _ = std::fs::write(app.workspace.config_path(), content);
                    app.config_content = app.workspace.config_content();
                }
            }
            Err(e) => {
                eprintln!("Failed to update auto-start: {}", e);
                // Revert the checkbox state
                app.config.general.start_with_windows = !start_with_windows;
            }
        }
    }

    ui.label(
        egui::RichText::new("When enabled, TaskPilot will start minimized to system tray on Windows startup")
            .small()
            .color(MUTED)
    );

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Updates section
    ui.label(egui::RichText::new("⬆ Updates").strong());
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
            .monospace()
            .small(),
    );
    ui.add_space(4.0);

    let mut auto_check = app.config.updates.auto_check;
    if ui.checkbox(&mut auto_check, "Check for updates automatically").changed() {
        app.config.updates.auto_check = auto_check;
        if let Ok(content) = toml::to_string_pretty(&app.config) {
            let _ = std::fs::write(app.workspace.config_path(), content);
            app.config_content = app.workspace.config_content();
        }
    }
    ui.label(
        egui::RichText::new("Checks GitHub for new releases every 24 hours")
            .small()
            .color(MUTED),
    );
    ui.add_space(8.0);

    match app.update_progress.clone() {
        UpdateProgress::Idle => {
            if ui.button("🔍 Check for Updates").clicked() {
                app.trigger_update_check();
            }
        }
        UpdateProgress::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking for updates...");
            });
        }
        UpdateProgress::Available(ver) => {
            ui.label(
                egui::RichText::new(format!("✓ Version v{} is available!", ver))
                    .color(GREEN),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("⬇ Download & Apply").clicked() {
                    app.trigger_update_apply();
                }
                if ui.button("🔍 Check Again").clicked() {
                    app.trigger_update_check();
                }
            });
        }
        UpdateProgress::Downloading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Downloading update...");
            });
        }
        UpdateProgress::ReadyToRestart(ver) => {
            ui.label(
                egui::RichText::new(format!("✓ v{} installed!", ver))
                    .color(GREEN)
                    .strong(),
            );
            ui.label("Restart TaskPilot to complete the update.");
            ui.add_space(4.0);
            if ui.button("🔄 Restart Now").clicked() {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .args(std::env::args().skip(1))
                        .spawn();
                    std::process::exit(0);
                }
            }
        }
        UpdateProgress::Error(msg) => {
            ui.label(egui::RichText::new(format!("⚠ {}", msg)).color(RED));
            ui.add_space(4.0);
            if ui.button("🔍 Try Again").clicked() {
                app.trigger_update_check();
            }
        }
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Config file section
    let config_path = app.workspace.config_path();
    ui.label(egui::RichText::new("📝 Configuration File").strong());
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("📄 {}", config_path.display()))
            .monospace()
            .small(),
    );
    ui.add_space(8.0);

    let mut should_reload = false;
    ui.horizontal(|ui| {
        if ui.button("📂 Open in Editor").clicked() {
            open_file_in_editor(&config_path);
        }
        if ui.button("🔄 Reload Config").clicked() {
            should_reload = true;
        }
    });

    if should_reload {
        app.reload_config();
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Config content display
    ui.label(egui::RichText::new("Configuration Preview").strong());
    ui.add_space(8.0);
    let content = app.config_content.clone();
    egui::Frame::none()
        .fill(egui::Color32::from_gray(15))
        .rounding(4.0)
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(&content).monospace().size(13.0));
        });
}

pub fn render_notifications(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    ui.heading("Notifications");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Recent alerts and task status changes").color(MUTED));
    ui.add_space(12.0);

    if app.notifications.is_empty() {
        ui.label(egui::RichText::new("No notifications yet. Run some tasks to see activity here.").color(MUTED));
        return;
    }

    let notifications = app.notifications.clone();
    for notif in &notifications {
        let (icon, color) = match notif.status {
            RunStatus::Passed => ("✓", GREEN),
            RunStatus::Failed => ("✕", RED),
            RunStatus::Timeout => ("⏱", YELLOW),
            RunStatus::Running => ("●", BLUE),
        };

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(color).heading());
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&notif.message).strong());
                        ui.label(
                            egui::RichText::new(
                                notif.time.format("%Y-%m-%d %H:%M:%S").to_string(),
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                });
            });
        ui.add_space(4.0);
    }
}

fn open_file_in_editor(path: &std::path::Path) {
    let path_str = path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path_str])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path_str).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&path_str)
            .spawn();
    }
}
