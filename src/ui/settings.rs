use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::TaskPilotApp;
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
    ui.label(egui::RichText::new("Recent alerts and job status changes").color(MUTED));
    ui.add_space(12.0);

    if app.notifications.is_empty() {
        ui.label(egui::RichText::new("No notifications yet. Run some jobs to see activity here.").color(MUTED));
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
