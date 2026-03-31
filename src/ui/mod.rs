mod dashboard;
mod task_detail;
mod settings;
mod sidebar;

use crate::app::{TaskPilotApp, View};
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
        egui::ScrollArea::vertical().show(ui, |ui| match view {
            View::Tasks => dashboard::render(app, ui),
            View::TaskDetail(ref name) => task_detail::render(app, ui, name),
            View::Settings => settings::render_settings(app, ui),
            View::Notifications => settings::render_notifications(app, ui),
        });
    });
}
