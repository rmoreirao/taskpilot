mod dashboard;
mod task_detail;
mod settings;
mod sidebar;

use crate::app::{TaskPilotApp, View};
use crate::config_diagnostics::ConfigIssueSeverity;
use eframe::egui;

pub(crate) const GREEN: egui::Color32 = egui::Color32::from_rgb(0, 184, 148);
pub(crate) const RED: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);
pub(crate) const BLUE: egui::Color32 = egui::Color32::from_rgb(9, 132, 227);
pub(crate) const YELLOW: egui::Color32 = egui::Color32::from_rgb(243, 156, 18);
pub(crate) const MUTED: egui::Color32 = egui::Color32::from_rgb(139, 144, 160);

pub fn render(app: &mut TaskPilotApp, ctx: &egui::Context) {
    sidebar::render(app, ctx);

    let view = app.current_view.clone();
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_config_banner(app, ui);
            match view {
                View::Tasks => dashboard::render(app, ui),
                View::TaskDetail(ref name) => task_detail::render(app, ui, name),
                View::Settings => settings::render_settings(app, ui),
                View::Notifications => settings::render_notifications(app, ui),
            }
        });
    });
}

fn render_config_banner(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    let Some(alert) = app.config_alert.clone() else {
        return;
    };
    if app.config_alert_dismissed {
        return;
    }

    let (fill, accent, label) = match alert.severity {
        ConfigIssueSeverity::Error => (
            egui::Color32::from_rgb(50, 30, 30),
            RED,
            "Config error",
        ),
        ConfigIssueSeverity::Warning => (
            egui::Color32::from_rgb(55, 45, 25),
            YELLOW,
            "Config warning",
        ),
    };

    egui::Frame::none()
        .fill(fill)
        .rounding(6.0)
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("⚠ {}: {}", label, alert.headline))
                            .color(accent)
                            .strong(),
                    );
                    if !alert.recovery.is_empty() {
                        ui.label(egui::RichText::new(&alert.recovery).color(MUTED));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.small_button("✕").clicked() {
                        app.config_alert_dismissed = true;
                    }
                });
            });

            ui.add_space(8.0);
            let visible_issues = 4usize;
            for issue in alert.issues.iter().take(visible_issues) {
                ui.label(
                    egui::RichText::new(format!("• {} — {}", issue.source, issue.message))
                        .small()
                        .color(egui::Color32::WHITE),
                );
            }
            if alert.issues.len() > visible_issues {
                ui.label(
                    egui::RichText::new(format!(
                        "… and {} more issue(s) in the logs.",
                        alert.issues.len() - visible_issues
                    ))
                    .small()
                    .color(MUTED),
                );
            }
        });
    ui.add_space(8.0);
}
