use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::{TaskPilotApp, View};
use crate::workspace::{RunStatus, Workspace};
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

        // Metadata cards — computed from all runs (not just current page)
        let all_runs = &app.selected_task_all_runs;
        let total_runs = all_runs.len();
        let passed_runs = all_runs.iter().filter(|r| r.status == RunStatus::Passed).count();
        let success_rate = if total_runs > 0 {
            passed_runs as f64 / total_runs as f64 * 100.0
        } else {
            0.0
        };
        let durations: Vec<u64> = all_runs.iter().filter_map(|r| r.duration_ms).collect();
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
                .map(|info| info.since.elapsed().as_secs_f64())
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

    // --- Execution History header with filter controls ---
    let mut filter_changed = false;
    let mut page_changed = false;

    ui.horizontal(|ui| {
        ui.strong("Execution History");
        ui.add_space(16.0);

        // Status filter
        ui.label(egui::RichText::new("Filter:").small().color(MUTED));
        let current_label = match &app.run_status_filter {
            None => "All",
            Some(RunStatus::Passed) => "Passed",
            Some(RunStatus::Failed) => "Failed",
            Some(RunStatus::Timeout) => "Timeout",
            Some(RunStatus::Stopped) => "Stopped",
            Some(RunStatus::Running) => "Running",
        };
        egui::ComboBox::from_id_source("run_status_filter")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut app.run_status_filter, None, "All").changed() {
                    filter_changed = true;
                }
                if ui.selectable_value(&mut app.run_status_filter, Some(RunStatus::Passed), "✓ Passed").changed() {
                    filter_changed = true;
                }
                if ui.selectable_value(&mut app.run_status_filter, Some(RunStatus::Failed), "✕ Failed").changed() {
                    filter_changed = true;
                }
                if ui.selectable_value(&mut app.run_status_filter, Some(RunStatus::Timeout), "⏱ Timeout").changed() {
                    filter_changed = true;
                }
                if ui.selectable_value(&mut app.run_status_filter, Some(RunStatus::Stopped), "■ Stopped").changed() {
                    filter_changed = true;
                }
            });

        ui.add_space(16.0);

        // Page size selector
        ui.label(egui::RichText::new("Per page:").small().color(MUTED));
        let current_per_page = app.runs_per_page.to_string();
        egui::ComboBox::from_id_source("runs_per_page")
            .selected_text(&current_per_page)
            .width(50.0)
            .show_ui(ui, |ui| {
                for &size in &[10usize, 15, 25, 50] {
                    if ui.selectable_value(&mut app.runs_per_page, size, size.to_string()).changed() {
                        filter_changed = true;
                    }
                }
            });
    });

    // Reset page when filter or page size changes
    if filter_changed {
        app.run_page = 0;
        app.expanded_run_outputs.clear();
        let (page_runs, total) = TaskPilotApp::paginate_runs(
            &app.selected_task_all_runs,
            app.run_status_filter.as_ref(),
            app.run_page,
            app.runs_per_page,
        );
        app.selected_task_runs = page_runs;
        app.selected_task_runs_total = total;
    }

    ui.add_space(8.0);

    if app.selected_task_runs.is_empty() && app.selected_task_all_runs.is_empty() {
        ui.label(egui::RichText::new("No runs recorded yet.").color(MUTED));
        return;
    }

    if app.selected_task_runs.is_empty() {
        ui.label(egui::RichText::new("No runs match the current filter.").color(MUTED));
    }

    // --- Pagination info ---
    let total_pages = if app.selected_task_runs_total == 0 {
        1
    } else {
        (app.selected_task_runs_total + app.runs_per_page - 1) / app.runs_per_page
    };

    if app.selected_task_runs_total > app.runs_per_page {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "Showing {} of {} runs",
                    app.selected_task_runs.len(),
                    app.selected_task_runs_total
                ))
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(4.0);
    }

    // Timeline of runs (current page only)
    let runs = app.selected_task_runs.clone();
    for run in &runs {
        let (icon, color) = match run.status {
            RunStatus::Passed => ("✓", GREEN),
            RunStatus::Failed => ("✕", RED),
            RunStatus::Timeout => ("⏱", YELLOW),
            RunStatus::Running => ("●", BLUE),
            RunStatus::Stopped => ("■", YELLOW),
        };

        let run_key = run.started_at.format("%Y-%m-%dT%H%M%S%.3f").to_string();

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
                    .id_source(format!("config-{}", run_key))
                    .show(ui, |ui| {
                        egui::Grid::new(format!("config-grid-{}", run_key))
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

                // Collapsible output — lazy-loaded on expand
                let has_output_log = run.output_log_path.is_some();
                let has_legacy_output = !run.stdout.is_empty() || !run.stderr.is_empty();
                if has_output_log || has_legacy_output {
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new(
                        egui::RichText::new("📄 Output").small().color(MUTED),
                    )
                    .id_source(format!("output-{}", run_key))
                    .default_open(false)
                    .show(ui, |ui| {
                        // Lazy-load: check cache first, then load from disk
                        let content = if let Some(cached) = app.expanded_run_outputs.get(&run_key) {
                            cached.clone()
                        } else {
                            let loaded = if let Some(ref path) = run.output_log_path {
                                Workspace::read_output_log_from_path(path)
                            } else {
                                let mut s = String::new();
                                if !run.stderr.is_empty() {
                                    s.push_str(&run.stderr);
                                }
                                if !run.stdout.is_empty() {
                                    if !s.is_empty() { s.push('\n'); }
                                    s.push_str(&run.stdout);
                                }
                                s
                            };
                            app.expanded_run_outputs.insert(run_key.clone(), loaded.clone());
                            loaded
                        };

                        if !content.is_empty() {
                            // Truncate to last 200 lines for display
                            let display_text: String = {
                                let lines: Vec<&str> = content.lines().collect();
                                if lines.len() > 200 {
                                    format!(
                                        "… ({} lines omitted)\n{}",
                                        lines.len() - 200,
                                        lines[lines.len() - 200..].join("\n")
                                    )
                                } else {
                                    content
                                }
                            };
                            egui::Frame::none()
                                .fill(egui::Color32::from_gray(20))
                                .rounding(4.0)
                                .inner_margin(egui::Margin::same(8.0))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(300.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(&display_text).monospace().small(),
                                            );
                                        });
                                });
                        }
                    });
                }
            });

        ui.add_space(4.0);
    }

    // --- Pagination controls ---
    if total_pages > 1 {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let on_first = app.run_page == 0;
            let on_last = app.run_page + 1 >= total_pages;

            if ui.add_enabled(!on_first, egui::Button::new("← Prev")).clicked() {
                app.run_page = app.run_page.saturating_sub(1);
                page_changed = true;
            }

            ui.label(format!("Page {} of {}", app.run_page + 1, total_pages));

            if ui.add_enabled(!on_last, egui::Button::new("Next →")).clicked() {
                app.run_page += 1;
                page_changed = true;
            }
        });
    }

    // Apply page change
    if page_changed {
        app.expanded_run_outputs.clear();
        let (page_runs, total) = TaskPilotApp::paginate_runs(
            &app.selected_task_all_runs,
            app.run_status_filter.as_ref(),
            app.run_page,
            app.runs_per_page,
        );
        app.selected_task_runs = page_runs;
        app.selected_task_runs_total = total;
    }
}
