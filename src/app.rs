use crate::config::AppConfig;
use crate::scheduler::{SchedulerCommand, SchedulerEvent, SchedulerHandle, start_scheduler};
use crate::tray::{TrayEvent, TrayManager};
use crate::workspace::{JobRun, RunStatus, Workspace};
use eframe::egui;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Dashboard,
    JobDetail(String),
    Settings,
    Notifications,
}

#[derive(Clone)]
pub struct JobStatus {
    pub name: String,
    pub command: String,
    pub cron: String,
    pub last_run: Option<JobRun>,
    pub is_running: bool,
}

#[derive(Clone)]
pub struct NotificationItem {
    pub job_name: String,
    pub message: String,
    pub status: RunStatus,
    pub time: chrono::DateTime<chrono::Utc>,
}

pub struct TaskPilotApp {
    pub(crate) workspace: Arc<Workspace>,
    pub(crate) config: AppConfig,
    pub(crate) config_content: String,
    scheduler: SchedulerHandle,
    pub(crate) current_view: View,
    pub(crate) job_statuses: Vec<JobStatus>,
    pub(crate) running_jobs: HashSet<String>,
    pub(crate) selected_job_runs: Vec<JobRun>,
    last_refresh: Instant,
    pub(crate) notifications: Vec<NotificationItem>,
    pub(crate) search_filter: String,
    tray: TrayManager,
    should_quit: bool,
}

impl TaskPilotApp {
    pub fn new(workspace: Arc<Workspace>, config: AppConfig, tray: TrayManager) -> Self {
        let config_content = workspace.config_content();
        let scheduler = start_scheduler(config.clone(), Arc::clone(&workspace));

        let mut app = Self {
            workspace,
            config,
            config_content,
            scheduler,
            current_view: View::Dashboard,
            job_statuses: Vec::new(),
            running_jobs: HashSet::new(),
            selected_job_runs: Vec::new(),
            last_refresh: Instant::now(),
            notifications: Vec::new(),
            search_filter: String::new(),
            tray,
            should_quit: false,
        };
        app.refresh_job_statuses();
        app
    }

    pub(crate) fn refresh_job_statuses(&mut self) {
        self.job_statuses = self
            .config
            .jobs
            .iter()
            .map(|job| {
                let last_run = self.workspace.get_latest_run(&job.name);
                JobStatus {
                    name: job.name.clone(),
                    command: job.command.clone(),
                    cron: job.cron.clone(),
                    last_run,
                    is_running: self.running_jobs.contains(&job.name),
                }
            })
            .collect();
    }

    fn process_events(&mut self) {
        while let Ok(evt) = self.scheduler.evt_rx.try_recv() {
            match evt {
                SchedulerEvent::JobStarted(name) => {
                    self.running_jobs.insert(name);
                }
                SchedulerEvent::JobFinished(name, status) => {
                    self.running_jobs.remove(&name);
                    let status_text = match &status {
                        RunStatus::Passed => "passed",
                        RunStatus::Failed => "failed",
                        RunStatus::Timeout => "timed out",
                        RunStatus::Running => "running",
                    };
                    self.notifications.insert(
                        0,
                        NotificationItem {
                            message: format!("{} {}", name, status_text),
                            job_name: name,
                            status,
                            time: chrono::Utc::now(),
                        },
                    );
                    self.notifications.truncate(50);
                }
            }
        }
    }

    pub(crate) fn trigger_job(&self, name: &str) {
        let _ = self
            .scheduler
            .cmd_tx
            .send(SchedulerCommand::RunJob(name.to_string()));
    }

    pub(crate) fn reload_config(&mut self) {
        match AppConfig::load(&self.workspace.config_path()) {
            Ok(new_config) => {
                self.config = new_config.clone();
                self.config_content = self.workspace.config_content();
                let _ = self
                    .scheduler
                    .cmd_tx
                    .send(SchedulerCommand::UpdateConfig(new_config));
            }
            Err(e) => {
                eprintln!("Failed to reload config: {}", e);
            }
        }
    }

    pub fn quit_app(&mut self) {
        self.should_quit = true;
    }

    fn process_tray_events(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.tray.check_event() {
            match event {
                TrayEvent::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::Quit => {
                    self.quit_app();
                }
            }
        }
    }
}

impl eframe::App for TaskPilotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events();
        self.process_tray_events(ctx);

        // If quit was requested (e.g. from tray menu), initiate window close
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Handle close request - hide window instead of closing
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.should_quit {
                // Actually quit the app
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                // Hide window instead of closing
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        // Auto-refresh from disk every 2 seconds
        if self.last_refresh.elapsed().as_secs() >= 2 {
            self.refresh_job_statuses();
            if let View::JobDetail(ref name) = self.current_view {
                self.selected_job_runs = self.workspace.load_runs(name, 50);
            }
            self.last_refresh = Instant::now();
        }

        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        crate::ui::render(self, ctx);
    }
}

impl Drop for TaskPilotApp {
    fn drop(&mut self) {
        let _ = self.scheduler.cmd_tx.send(SchedulerCommand::Shutdown);
    }
}
