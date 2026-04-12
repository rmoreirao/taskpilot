use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::{TaskPilotApp, UpdateProgress, View};
use crate::workspace::RunStatus;
use eframe::egui;

pub fn render(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    // Update banner
    if !app.update_banner_dismissed {
        render_update_banner(app, ui);
    }

    ui.heading("Tasks");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Overview of all scheduled tasks").color(MUTED));
    ui.add_space(12.0);

    // Stats cards
    let total = app.task_statuses.len();
    let passed = app
        .task_statuses
        .iter()
        .filter(|t| {
            t.last_run
                .as_ref()
                .map_or(false, |r| r.status == RunStatus::Passed)
        })
        .count();
    let failed = app
        .task_statuses
        .iter()
        .filter(|t| {
            t.last_run.as_ref().map_or(false, |r| {
                r.status == RunStatus::Failed || r.status == RunStatus::Timeout
            })
        })
        .count();
    let running = app.running_tasks.len();

    ui.horizontal(|ui| {
        stat_card(ui, "Total Tasks", total, egui::Color32::WHITE);
        stat_card(ui, "Passed", passed, GREEN);
        stat_card(ui, "Failed", failed, RED);
        stat_card(ui, "Running", running, BLUE);
    });

    ui.add_space(16.0);

    // Search filter
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(egui::TextEdit::singleline(&mut app.search_filter).hint_text("Filter tasks..."));
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Task table
    let filter = app.search_filter.to_lowercase();
    let tasks: Vec<_> = app
        .task_statuses
        .iter()
        .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(&filter))
        .cloned()
        .collect();

    let mut trigger_task = None;
    let mut stop_task = None;
    let mut view_task = None;

    egui::Grid::new("task_table")
        .striped(true)
        .min_col_width(60.0)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            // Header
            ui.strong("Task Name");
            ui.strong("Source");
            ui.strong("Schedule");
            ui.strong("Last Run");
            ui.strong("Status");
            ui.strong("Duration");
            ui.strong("Next Run");
            ui.strong("Actions");
            ui.end_row();

            for task in &tasks {
                // Name (clickable link)
                if ui.link(&task.name).clicked() {
                    view_task = Some(task.name.clone());
                }

                // Source badge
                if let Some(info) = app.source_metadata.get(&task.name) {
                    if info.is_external() {
                        ui.label(
                            egui::RichText::new(format!("📁 {}", info.source_label()))
                                .small()
                                .color(YELLOW),
                        );
                    } else {
                        ui.label(egui::RichText::new("local").small().color(MUTED));
                    }
                } else {
                    ui.label(egui::RichText::new("local").small().color(MUTED));
                }

                // Cron schedule
                ui.label(egui::RichText::new(&task.cron).monospace().color(BLUE));

                // Last run time
                if let Some(run) = &task.last_run {
                    ui.label(
                        egui::RichText::new(run.started_at.format("%Y-%m-%d %H:%M").to_string())
                            .color(MUTED),
                    );
                } else {
                    ui.label(egui::RichText::new("—").color(MUTED));
                }

                // Status
                if task.is_running {
                    ui.label(egui::RichText::new("● Running").color(BLUE));
                } else if let Some(run) = &task.last_run {
                    match run.status {
                        RunStatus::Passed => {
                            ui.label(egui::RichText::new("✓ Passed").color(GREEN));
                        }
                        RunStatus::Failed => {
                            ui.label(egui::RichText::new("✕ Failed").color(RED));
                        }
                        RunStatus::Timeout => {
                            ui.label(egui::RichText::new("⏱ Timeout").color(YELLOW));
                        }
                        RunStatus::Running => {
                            ui.label(egui::RichText::new("● Running").color(BLUE));
                        }
                        RunStatus::Stopped => {
                            ui.label(egui::RichText::new("■ Stopped").color(YELLOW));
                        }
                    };
                } else {
                    ui.label(egui::RichText::new("— Never run").color(MUTED));
                }

                // Duration (live elapsed for running tasks)
                if task.is_running {
                    if let Some(started) = task.running_since {
                        let secs = started.elapsed().as_secs_f64();
                        ui.label(
                            egui::RichText::new(format!("{:.1}s", secs))
                                .monospace()
                                .color(BLUE),
                        );
                    } else {
                        ui.label(egui::RichText::new("…").color(BLUE));
                    }
                } else if let Some(run) = &task.last_run {
                    if let Some(ms) = run.duration_ms {
                        let secs = ms as f64 / 1000.0;
                        ui.label(
                            egui::RichText::new(format!("{:.1}s", secs))
                                .monospace()
                                .color(MUTED),
                        );
                    } else {
                        ui.label(egui::RichText::new("—").color(MUTED));
                    }
                } else {
                    ui.label(egui::RichText::new("—").color(MUTED));
                }

                // Next Run
                if let Some(next) = task.next_run {
                    ui.label(
                        egui::RichText::new(next.format("%Y-%m-%d %H:%M").to_string())
                            .color(MUTED),
                    );
                } else {
                    ui.label(egui::RichText::new("—").color(MUTED));
                }

                // Actions
                if task.is_running {
                    if ui
                        .button(egui::RichText::new("■ Stop").color(RED))
                        .clicked()
                    {
                        stop_task = Some(task.name.clone());
                    }
                } else if ui.button("▶ Run").clicked() {
                    trigger_task = Some(task.name.clone());
                }

                ui.end_row();
            }
        });

    // Apply actions after rendering
    if let Some(name) = trigger_task {
        app.trigger_task(&name);
    }
    if let Some(name) = stop_task {
        app.stop_task(&name);
    }
    if let Some(name) = view_task {
        app.run_page = 0;
        app.run_status_filter = None;
        app.expanded_run_outputs.clear();
        app.expanded_runs.clear();
        app.load_task_detail_runs(&name);
        app.current_view = View::TaskDetail(name);
    }
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: usize, color: egui::Color32) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).small().color(MUTED));
                ui.label(egui::RichText::new(value.to_string()).heading().color(color));
            });
        });
}

fn render_update_banner(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    match app.update_progress.clone() {
        UpdateProgress::Available(ver) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 50, 30))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("⬆ Update available: v{}", ver))
                                .color(GREEN)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                app.update_banner_dismissed = true;
                            }
                            if ui.button("Update Now").clicked() {
                                app.trigger_update_apply();
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }
        UpdateProgress::Downloading => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 40, 55))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("⏳ Downloading update...")
                            .color(BLUE),
                    );
                });
            ui.add_space(8.0);
        }
        UpdateProgress::ReadyToRestart(ver) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 50, 30))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "✓ v{} installed — restart to complete the update",
                                ver
                            ))
                            .color(GREEN)
                            .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                app.update_banner_dismissed = true;
                            }
                            if ui.button("Restart").clicked() {
                                restart_app();
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }
        UpdateProgress::Error(msg) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(50, 30, 30))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("⚠ Update error: {}", msg))
                                .color(RED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                app.update_banner_dismissed = true;
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }
        _ => {}
    }
}

fn restart_app() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .args(std::env::args().skip(1))
            .spawn();
        std::process::exit(0);
    }
}
