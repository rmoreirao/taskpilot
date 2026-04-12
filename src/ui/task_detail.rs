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

    // Running status badge
    let is_running = app.running_tasks.contains_key(task_name);
    if is_running {
        ui.horizontal(|ui| {
            let elapsed = app
                .running_tasks
                .get(task_name)
                .map(|started| started.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            ui.label(
                egui::RichText::new(format!("● Running — {:.1}s elapsed", elapsed))
                    .color(BLUE)
                    .strong(),
            );
        });
        ui.add_space(8.0);
    }

    // Run Now button (disabled while running)
    let task_name_owned = task_name.to_string();
    ui.add_enabled_ui(!is_running, |ui| {
        if ui.button("▶ Run Now").clicked() {
            app.trigger_task(&task_name_owned);
        }
    });

    // Live output section with refresh controls
    if is_running {
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Refresh toolbar
        ui.horizontal(|ui| {
            ui.strong("Live Output");
            ui.add_space(16.0);

            if ui.button("🔄 Refresh").clicked() {
                app.force_log_refresh = true;
            }

            ui.add_space(16.0);
            ui.label(egui::RichText::new("Auto-refresh:").small().color(MUTED));
            let slider = egui::Slider::new(&mut app.log_refresh_interval_secs, 1.0..=30.0)
                .step_by(1.0)
                .suffix("s");
            ui.add(slider);
        });

        ui.add_space(4.0);

        let live_content = app.live_logs.get(task_name).cloned().unwrap_or_default();

        if live_content.is_empty() {
            ui.label(egui::RichText::new("Waiting for output…").color(MUTED));
        } else {
            let display_text: String = {
                let lines: Vec<&str> = live_content.lines().collect();
                if lines.len() > 200 {
                    lines[lines.len() - 200..].join("\n")
                } else {
                    live_content.clone()
                }
            };

            egui::Frame::none()
                .fill(egui::Color32::from_gray(20))
                .rounding(4.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&display_text).monospace().small());
                        });
                });
        }
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
            RunStatus::Stopped => ("■", YELLOW),
        };

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(color).strong());
                    ui.label(egui::RichText::new(format!("{:?}", run.status)).color(color));
                    // Show attempt info if retries were configured
                    if let (Some(attempt), Some(total)) = (run.attempt, run.total_attempts) {
                        if total > 1 {
                            ui.label(
                                egui::RichText::new(format!("(attempt {}/{})", attempt + 1, total))
                                    .small()
                                    .color(MUTED),
                            );
                        }
                    }
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

                // Show task config snapshot if available
                if let Some(ref cfg) = run.config {
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new(
                        egui::RichText::new("📋 Task Config").small().color(MUTED),
                    )
                    .id_source(format!("config-{}", run.started_at.timestamp_millis()))
                    .show(ui, |ui| {
                        egui::Grid::new(format!("config-grid-{}", run.started_at.timestamp_millis()))
                            .num_columns(2)
                            .spacing([12.0, 4.0])
                            .show(ui, |ui| {
                                let field = |ui: &mut egui::Ui, label: &str, value: &str| {
                                    ui.label(egui::RichText::new(label).small().color(MUTED));
                                    ui.label(egui::RichText::new(value).small().monospace());
                                    ui.end_row();
                                };
                                field(ui, "Command:", &cfg.command);
                                field(ui, "Cron:", &cfg.cron);
                                field(ui, "Shell:", &cfg.shell);
                                field(ui, "Timeout:", cfg.timeout.as_deref().unwrap_or("none"));
                                field(ui, "Working Dir:", cfg.working_dir.as_deref().unwrap_or("(default)"));
                                field(ui, "Retries:", &cfg.retries.to_string());
                                field(ui, "Notify on Failure:", if cfg.notify_on_failure { "yes" } else { "no" });
                                field(ui, "Run Missed:", if cfg.run_missed { "yes" } else { "no" });
                            });
                    });
                }

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
