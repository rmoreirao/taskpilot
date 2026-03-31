use super::{BLUE, GREEN, MUTED, RED, YELLOW};
use crate::app::{TaskPilotApp, View};
use crate::workspace::RunStatus;
use eframe::egui;

pub fn render(app: &mut TaskPilotApp, ui: &mut egui::Ui) {
    ui.heading("Dashboard");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Overview of all scheduled jobs").color(MUTED));
    ui.add_space(12.0);

    // Stats cards
    let total = app.job_statuses.len();
    let passed = app
        .job_statuses
        .iter()
        .filter(|j| {
            j.last_run
                .as_ref()
                .map_or(false, |r| r.status == RunStatus::Passed)
        })
        .count();
    let failed = app
        .job_statuses
        .iter()
        .filter(|j| {
            j.last_run.as_ref().map_or(false, |r| {
                r.status == RunStatus::Failed || r.status == RunStatus::Timeout
            })
        })
        .count();
    let running = app.running_jobs.len();

    ui.horizontal(|ui| {
        stat_card(ui, "Total Jobs", total, egui::Color32::WHITE);
        stat_card(ui, "Passed", passed, GREEN);
        stat_card(ui, "Failed", failed, RED);
        stat_card(ui, "Running", running, BLUE);
    });

    ui.add_space(16.0);

    // Search filter
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(egui::TextEdit::singleline(&mut app.search_filter).hint_text("Filter jobs..."));
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Job table
    let filter = app.search_filter.to_lowercase();
    let jobs: Vec<_> = app
        .job_statuses
        .iter()
        .filter(|j| filter.is_empty() || j.name.to_lowercase().contains(&filter))
        .cloned()
        .collect();

    let mut trigger_job = None;
    let mut view_job = None;

    egui::Grid::new("job_table")
        .striped(true)
        .min_col_width(60.0)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            // Header
            ui.strong("Job Name");
            ui.strong("Schedule");
            ui.strong("Last Run");
            ui.strong("Status");
            ui.strong("Duration");
            ui.strong("Actions");
            ui.end_row();

            for job in &jobs {
                // Name (clickable link)
                if ui.link(&job.name).clicked() {
                    view_job = Some(job.name.clone());
                }

                // Cron schedule
                ui.label(egui::RichText::new(&job.cron).monospace().color(BLUE));

                // Last run time
                if let Some(run) = &job.last_run {
                    ui.label(
                        egui::RichText::new(run.started_at.format("%H:%M:%S").to_string())
                            .color(MUTED),
                    );
                } else {
                    ui.label(egui::RichText::new("—").color(MUTED));
                }

                // Status
                if job.is_running {
                    ui.label(egui::RichText::new("● Running").color(BLUE));
                } else if let Some(run) = &job.last_run {
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
                    };
                } else {
                    ui.label(egui::RichText::new("— Never run").color(MUTED));
                }

                // Duration
                if let Some(run) = &job.last_run {
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

                // Actions
                if job.is_running {
                    ui.label(egui::RichText::new("⏳ Running...").color(MUTED));
                } else if ui.button("▶ Run").clicked() {
                    trigger_job = Some(job.name.clone());
                }

                ui.end_row();
            }
        });

    // Apply actions after rendering
    if let Some(name) = trigger_job {
        app.trigger_job(&name);
    }
    if let Some(name) = view_job {
        app.selected_job_runs = app.workspace.load_runs(&name, 50);
        app.current_view = View::JobDetail(name);
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
