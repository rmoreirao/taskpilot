use super::{GREEN, MUTED, YELLOW};
use crate::app::{TaskPilotApp, UpdateProgress, View};
use eframe::egui;

pub fn render(app: &mut TaskPilotApp, ctx: &egui::Context) {
    egui::SidePanel::left("sidebar")
        .exact_width(200.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("⚡");
                ui.heading(egui::RichText::new("TaskPilot").strong());
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            let nav_items = [
                ("📊  Tasks", View::Tasks),
                ("⚙\u{fe0f}  Settings", View::Settings),
            ];

            for (label, target_view) in &nav_items {
                let selected = std::mem::discriminant(&app.current_view)
                    == std::mem::discriminant(target_view);
                if ui.selectable_label(selected, *label).clicked() {
                    app.current_view = target_view.clone();
                }
            }

            // Notifications with count badge
            let notif_count = app.notifications.len();
            let notif_label = if notif_count > 0 {
                format!("🔔  Notifications ({})", notif_count)
            } else {
                "🔔  Notifications".to_string()
            };
            let selected = matches!(app.current_view, View::Notifications);
            if ui.selectable_label(selected, &notif_label).clicked() {
                app.current_view = View::Notifications;
            }

            // Footer
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("● Engine running").small().color(GREEN));
                ui.label(
                    egui::RichText::new(format!("{} tasks configured", app.config.tasks.len()))
                        .small()
                        .color(MUTED),
                );

                // Version with update indicator
                match &app.update_progress {
                    UpdateProgress::Available(ver) | UpdateProgress::ReadyToRestart(ver) => {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().color(MUTED));
                            ui.label(egui::RichText::new(format!("⬆ v{}", ver)).small().color(YELLOW));
                        });
                    }
                    UpdateProgress::Checking => {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().color(MUTED));
                            ui.label(egui::RichText::new("⟳").small().color(MUTED));
                        });
                    }
                    _ => {
                        ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().color(MUTED));
                    }
                }

                ui.separator();
            });
        });
}
