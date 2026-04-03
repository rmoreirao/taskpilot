use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::{TaskPilotApp, View};
use crate::workspace::RunStatus;
use eframe::egui;

pub fn render(app: &mut TaskPilotApp, ui: &mut egui::Ui, task_name: &str) {
    // Back button
    if ui.button("← Back to Tasks").clicked() {
        app.current_view = View::Tasks;
        return;
    }

    ui.add_space(8.0);
    ui.heading(task_name);

    // Find task config
    let task_config = app.config.tasks.iter().find(|t| t.name == task_name).cloned();

    if let Some(config) = &task_config {
        ui.label(egui::RichText::new(&config.command).monospace().color(MUTED));

        // Show source info for external tasks
        if let Some(info) = app.source_metadata.get(task_name) {
            if info.is_external() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "📁 External source: {}",
                            info.file_path.display()
                        ))
                        .small()
                        .color(YELLOW),
                    );
                });
            }
        }

        ui.add_space(12.0);

        // Metadata cards
        let runs = &app.selected_task_runs;
        let total_runs = runs.len();
        let passed_runs = runs.iter().filter(|r| r.status == RunStatus::Passed).count();
        let success_rate = if total_runs > 0 {
            passed_runs as f64 / total_runs as f64 * 100.0
        } else {
            0.0
        };
        let durations: Vec<u64> = runs.iter().filter_map(|r| r.duration_ms).collect();
        let avg_duration = if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<u64>() as f64 / durations.len() as f64 / 1000.0
        };

        ui.horizontal(|ui| {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Schedule").small().color(MUTED));
                    ui.label(egui::RichText::new(&config.cron).monospace().color(BLUE));
                });

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Success Rate").small().color(MUTED));
                    let color = if success_rate >= 90.0 {
                        GREEN
                    } else if success_rate >= 70.0 {
                        YELLOW
                    } else {
                        RED
                    };
                    ui.label(
                        egui::RichText::new(format!("{:.1}%", success_rate))
                            .color(color)
                            .strong(),
                    );
                });

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Avg Duration").small().color(MUTED));
                    ui.label(egui::RichText::new(format!("{:.1}s", avg_duration)).strong());
                });

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Total Runs").small().color(MUTED));
                    ui.label(egui::RichText::new(total_runs.to_string()).strong());
                });
        });
    }

    ui.add_space(12.0);

    // Run Now button
    let task_name_owned = task_name.to_string();
    if ui.button("▶ Run Now").clicked() {
        app.trigger_task(&task_name_owned);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.strong("Execution History");
    ui.add_space(8.0);

    if app.selected_task_runs.is_empty() {
        ui.label(egui::RichText::new("No runs recorded yet.").color(MUTED));
        return;
    }

    // Timeline of runs
    let runs = app.selected_task_runs.clone();
    for run in &runs {
        let (icon, color) = match run.status {
            RunStatus::Passed => ("✓", GREEN),
            RunStatus::Failed => ("✕", RED),
            RunStatus::Timeout => ("⏱", YELLOW),
            RunStatus::Running => ("●", BLUE),
        };

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(color).strong());
                    ui.label(egui::RichText::new(format!("{:?}", run.status)).color(color));
                    ui.label(
                        egui::RichText::new(
                            run.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                        )
                        .color(MUTED),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(ms) = run.duration_ms {
                            ui.label(
                                egui::RichText::new(format!("{:.1}s", ms as f64 / 1000.0))
                                    .monospace()
                                    .color(MUTED),
                            );
                        }
                        if let Some(code) = run.exit_code {
                            ui.label(
                                egui::RichText::new(format!("exit {}", code))
                                    .monospace()
                                    .color(MUTED),
                            );
                        }
                    });
                });

                // Show output
                if !run.stderr.is_empty() {
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(20))
                        .rounding(4.0)
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&run.stderr)
                                    .monospace()
                                    .small()
                                    .color(RED),
                            );
                        });
                }
                if !run.stdout.is_empty() {
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(20))
                        .rounding(4.0)
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&run.stdout).monospace().small());
                        });
                }
            });

        ui.add_space(4.0);
    }
}
