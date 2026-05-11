use crate::config::AppConfig;
use crate::config_diagnostics::ConfigAlert;
use crate::runtime_config::{self, ReloadConfigOutcome, RuntimeConfigState};
use crate::scheduler::{self, SchedulerCommand, SchedulerEvent, SchedulerHandle, start_scheduler};
use crate::single_instance::SingleInstanceGuard;
use crate::task_sources::TaskSourceInfo;
use crate::timezone;
use crate::tray::{TrayEvent, TrayManager};
use crate::updater::{self, UpdateState};
use crate::workspace::{TaskRun, RunStatus, Workspace};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Tasks,
    TaskDetail(String),
    Settings,
    Notifications,
}

#[derive(Clone)]
pub struct RunningTaskInfo {
    pub since: Instant,
    pub started_at: DateTime<Local>,
}

#[derive(Clone)]
pub struct TaskStatus {
    pub name: String,
    pub command: String,
    pub cron: String,
    pub schedule_timezone: Option<String>,
    pub last_run: Option<TaskRun>,
    pub next_run: Option<DateTime<Local>>,
    pub is_running: bool,
    pub running_since: Option<Instant>,
}

#[derive(Clone)]
pub struct NotificationItem {
    pub task_name: String,
    pub message: String,
    pub status: RunStatus,
    pub time: chrono::DateTime<chrono::Local>,
}

/// Tracks the state of an in-progress update operation.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateProgress {
    Idle,
    Checking,
    Available(String),
    Downloading,
    ReadyToRestart(String),
    Error(String),
}

pub struct TaskPilotApp {
    pub(crate) workspace: Arc<Workspace>,
    pub(crate) config: AppConfig,
    pub(crate) config_content: String,
    scheduler: SchedulerHandle,
    pub(crate) current_view: View,
    pub(crate) task_statuses: Vec<TaskStatus>,
    pub(crate) running_tasks: HashMap<String, RunningTaskInfo>,
    pub(crate) live_logs: HashMap<String, String>,
    pub(crate) selected_task_runs: Vec<TaskRun>,
    /// Total number of runs matching the current status filter (for pagination).
    pub(crate) selected_task_runs_total: usize,
    /// All run metadata (lightweight, no output) for computing summary stats.
    pub(crate) selected_task_all_runs: Vec<TaskRun>,
    pub(crate) run_page: usize,
    pub(crate) runs_per_page: usize,
    pub(crate) run_status_filter: Option<RunStatus>,
    /// Lazily-loaded output content, keyed by run timestamp string.
    pub(crate) expanded_run_outputs: HashMap<String, String>,
    /// Tracks which run entries are expanded in the execution history.
    pub(crate) expanded_runs: HashSet<String>,
    last_refresh: Instant,
    pub(crate) notifications: Vec<NotificationItem>,
    pub(crate) search_filter: String,
    tray: TrayManager,
    should_quit: bool,
    pub(crate) source_metadata: HashMap<String, TaskSourceInfo>,
    pub(crate) source_dirs: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
    cli_task_dirs: Vec<PathBuf>,
    _watcher: Option<notify::RecommendedWatcher>,
    watcher_rx: Option<std::sync::mpsc::Receiver<()>>,
    single_instance_guard: SingleInstanceGuard,
    pub(crate) log_refresh_interval_secs: f32,
    last_log_refresh: Instant,
    pub(crate) force_log_refresh: bool,
    // Update state
    pub(crate) update_state: UpdateState,
    pub(crate) update_progress: UpdateProgress,
    pub(crate) update_banner_dismissed: bool,
    pub(crate) config_alert: Option<ConfigAlert>,
    pub(crate) config_alert_dismissed: bool,
    update_check_rx: Option<std::sync::mpsc::Receiver<Result<UpdateState, String>>>,
    update_apply_rx: Option<std::sync::mpsc::Receiver<Result<updater::UpdateResult, String>>>,
    last_update_check: Option<Instant>,
}

impl TaskPilotApp {
    pub fn new(
        workspace: Arc<Workspace>,
        state: RuntimeConfigState,
        tray: TrayManager,
        cli_task_dirs: Vec<PathBuf>,
        single_instance_guard: SingleInstanceGuard,
    ) -> Self {
        let config_content = workspace.config_content();
        let scheduler = start_scheduler(state.config.clone(), Arc::clone(&workspace));

        // Set up file watcher for external source directories and individual files
        let (watcher, watcher_rx) = Self::create_watcher(&state.source_dirs, &state.source_files);

        // Clean up old binaries from a previous update
        updater::cleanup_old_binaries();

        // Load persisted update state
        let update_state_path = updater::update_state_path(&workspace.root);
        let update_state = UpdateState::load(&update_state_path);
        let update_progress = if update_state.has_update() {
            UpdateProgress::Available(
                update_state.available_version.clone().unwrap_or_default(),
            )
        } else {
            UpdateProgress::Idle
        };

        let mut app = Self {
            workspace,
            config: state.config,
            config_content,
            scheduler,
            current_view: View::Tasks,
            task_statuses: Vec::new(),
            running_tasks: HashMap::new(),
            live_logs: HashMap::new(),
            selected_task_runs: Vec::new(),
            selected_task_runs_total: 0,
            selected_task_all_runs: Vec::new(),
            run_page: 0,
            runs_per_page: 15,
            run_status_filter: None,
            expanded_run_outputs: HashMap::new(),
            expanded_runs: HashSet::new(),
            last_refresh: Instant::now(),
            notifications: Vec::new(),
            search_filter: String::new(),
            tray,
            should_quit: false,
            source_metadata: state.source_metadata,
            source_dirs: state.source_dirs,
            source_files: state.source_files,
            cli_task_dirs,
            _watcher: watcher,
            watcher_rx,
            single_instance_guard,
            log_refresh_interval_secs: 2.0,
            last_log_refresh: Instant::now(),
            force_log_refresh: false,
            update_state,
            update_progress,
            update_banner_dismissed: false,
            config_alert: state.config_alert,
            config_alert_dismissed: false,
            update_check_rx: None,
            update_apply_rx: None,
            last_update_check: None,
        };
        app.refresh_task_statuses();

        // Trigger an initial update check if auto_check is enabled and it's time
        if app.config.updates.auto_check && app.update_state.needs_check(24) {
            app.trigger_update_check();
        }

        let _ = app
            .workspace
            .append_debug_log("app", "TaskPilot application initialized");
        app
    }

    fn create_watcher(
        dirs: &[PathBuf],
        files: &[PathBuf],
    ) -> (
        Option<notify::RecommendedWatcher>,
        Option<std::sync::mpsc::Receiver<()>>,
    ) {
        use notify::{RecursiveMode, Watcher};

        if dirs.is_empty() && files.is_empty() {
            return (None, None);
        }

        let (tx, rx) = std::sync::mpsc::channel();

        // Debounce: forward events with a simple flag
        let notify_result = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            if let Ok(event) = res {
                // Only react to file modifications/creations/removals of .toml files
                let dominated_by_toml = event.paths.iter().any(|p| {
                    p.extension().map_or(false, |ext| ext == "toml")
                });
                if dominated_by_toml {
                    let _ = tx.send(());
                }
            }
        });

        match notify_result {
            Ok(mut watcher) => {
                for dir in dirs {
                    if dir.exists() {
                        let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
                    }
                }
                for file in files {
                    if file.exists() {
                        let _ = watcher.watch(file, RecursiveMode::NonRecursive);
                    }
                }
                (Some(watcher), Some(rx))
            }
            Err(e) => {
                eprintln!("Failed to create file watcher: {}", e);
                (None, None)
            }
        }
    }

    fn runtime_state(&self) -> RuntimeConfigState {
        RuntimeConfigState {
            config: self.config.clone(),
            source_metadata: self.source_metadata.clone(),
            source_dirs: self.source_dirs.clone(),
            source_files: self.source_files.clone(),
            config_alert: self.config_alert.clone(),
        }
    }

    fn set_config_alert(&mut self, alert: Option<ConfigAlert>) {
        self.config_alert = alert;
        self.config_alert_dismissed = false;
    }

    fn apply_config_state(&mut self, state: RuntimeConfigState) {
        let source_dirs_changed = state.source_dirs != self.source_dirs;
        let source_files_changed = state.source_files != self.source_files;

        self.config = state.config.clone();
        self.source_metadata = state.source_metadata;
        self.set_config_alert(state.config_alert);

        if source_dirs_changed || source_files_changed {
            let (watcher, watcher_rx) = Self::create_watcher(&state.source_dirs, &state.source_files);
            self._watcher = watcher;
            self.watcher_rx = watcher_rx;
            self.source_dirs = state.source_dirs;
            self.source_files = state.source_files;
        }

        self.refresh_task_statuses();
        let _ = self
            .scheduler
            .cmd_tx
            .send(SchedulerCommand::UpdateConfig(state.config));
    }

    /// Load task detail runs (metadata only) and paginate for the current view.
    pub(crate) fn load_task_detail_runs(&mut self, task_name: &str) {
        self.selected_task_all_runs = self.workspace.load_runs_metadata(task_name, 200);
        let (page_runs, total) = Self::paginate_runs(
            &self.selected_task_all_runs,
            self.run_status_filter.as_ref(),
            self.run_page,
            self.runs_per_page,
        );
        self.selected_task_runs = page_runs;
        self.selected_task_runs_total = total;
        self.expanded_run_outputs.clear();
        self.expanded_runs.clear();
    }

    /// Filter and paginate a list of runs. Returns (page_items, total_matching).
    pub(crate) fn paginate_runs(
        all_runs: &[TaskRun],
        status_filter: Option<&RunStatus>,
        page: usize,
        per_page: usize,
    ) -> (Vec<TaskRun>, usize) {
        let filtered: Vec<&TaskRun> = if let Some(status) = status_filter {
            all_runs.iter().filter(|r| &r.status == status).collect()
        } else {
            all_runs.iter().collect()
        };
        let total = filtered.len();
        let start = page * per_page;
        let page_runs: Vec<TaskRun> = filtered
            .into_iter()
            .skip(start)
            .take(per_page)
            .cloned()
            .collect();
        (page_runs, total)
    }

    pub(crate) fn refresh_task_statuses(&mut self) {
        let sched_state = self.workspace.load_state();
        self.task_statuses = self
            .config
            .tasks
            .iter()
            .map(|task| {
                let last_run = self.workspace.get_latest_run(&task.name);
                let next_run = sched_state
                    .tasks
                    .get(&task.name)
                    .and_then(|s| s.next_run);
                TaskStatus {
                    name: task.name.clone(),
                    command: task.command.clone(),
                    cron: task.cron.clone().unwrap_or_default(),
                    schedule_timezone: task.cron.as_ref().and_then(|_| {
                        timezone::effective_timezone_label(
                            task,
                            self.config.general.default_timezone.as_deref(),
                        )
                        .ok()
                    }),
                    last_run,
                    next_run,
                    is_running: self.running_tasks.contains_key(&task.name),
                    running_since: self.running_tasks.get(&task.name).map(|info| info.since),
                }
            })
            .collect();
    }

    fn process_events(&mut self) {
        while let Ok(evt) = self.scheduler.evt_rx.try_recv() {
            match evt {
                SchedulerEvent::TaskStarted(name, _trigger, started_at) => {
                    self.running_tasks.insert(name, RunningTaskInfo {
                        since: Instant::now(),
                        started_at,
                    });
                }
                SchedulerEvent::TaskFinished(name, status, trigger) => {
                    self.running_tasks.remove(&name);
                    self.live_logs.remove(&name);
                    let status_text = match &status {
                        RunStatus::Passed => "passed",
                        RunStatus::Failed => "failed",
                        RunStatus::Timeout => "timed out",
                        RunStatus::Running => "running",
                        RunStatus::Stopped => "stopped",
                    };
                    let trigger_prefix = match &trigger {
                        scheduler::TaskTrigger::CatchUp { scheduled_for } => {
                            format!("⏰ catch-up (due {}) — ", scheduled_for.format("%H:%M"))
                        }
                        scheduler::TaskTrigger::Triggered { source, source_status } => {
                            format!("🔗 triggered by '{}' ({:?}) — ", source, source_status)
                        }
                        _ => String::new(),
                    };
                    self.notifications.insert(
                        0,
                        NotificationItem {
                            message: format!("{}{} {}", trigger_prefix, name, status_text),
                            task_name: name,
                            status,
                            time: chrono::Local::now(),
                        },
                    );
                    self.notifications.truncate(50);
                }
                SchedulerEvent::TaskSkipped {
                    name,
                    scheduled_for,
                    reason,
                } => {
                    self.notifications.insert(
                        0,
                        NotificationItem {
                            message: format!(
                                "⏭️ {} skipped (due {}, {})",
                                name,
                                scheduled_for.format("%H:%M"),
                                reason
                            ),
                            task_name: name,
                            status: RunStatus::Stopped,
                            time: chrono::Local::now(),
                        },
                    );
                    self.notifications.truncate(50);
                }
            }
        }
    }

    pub(crate) fn trigger_task(&self, name: &str) {
        let _ = self
            .scheduler
            .cmd_tx
            .send(SchedulerCommand::RunTask(name.to_string()));
    }

    pub(crate) fn stop_task(&self, name: &str) {
        let _ = self
            .scheduler
            .cmd_tx
            .send(SchedulerCommand::StopTask(name.to_string()));
    }

    fn process_watcher_events(&mut self) {
        if let Some(rx) = &self.watcher_rx {
            // Drain all pending events (debounce by consuming all)
            let mut changed = false;
            while rx.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                let _ = self
                    .workspace
                    .append_debug_log("watcher", "External task source changed, reloading");
                self.reload_config();
            }
        }
    }

    pub(crate) fn reload_config(&mut self) {
        self.config_content = self.workspace.config_content();
        let ReloadConfigOutcome { state, applied } = runtime_config::load_reload_state(
            &self.workspace,
            &self.cli_task_dirs,
            &self.runtime_state(),
        );

        if applied {
            self.apply_config_state(state);
            let _ = self
                .workspace
                .append_debug_log("app", "Config reloaded successfully");
        } else {
            self.set_config_alert(state.config_alert);
        }
    }

    pub fn quit_app(&mut self) {
        let _ = self
            .workspace
            .append_debug_log("app", "Quit requested; shutting down");
        let _ = self.scheduler.cmd_tx.send(SchedulerCommand::Shutdown);
        std::process::exit(0);
    }

    /// Start a background update check.
    pub(crate) fn trigger_update_check(&mut self) {
        if matches!(self.update_progress, UpdateProgress::Checking | UpdateProgress::Downloading) {
            return; // Already in progress
        }
        self.update_progress = UpdateProgress::Checking;
        let (tx, rx) = std::sync::mpsc::channel();
        self.update_check_rx = Some(rx);
        self.last_update_check = Some(Instant::now());

        std::thread::spawn(move || {
            let result = updater::check_for_update();
            let _ = tx.send(result);
        });

        let _ = self
            .workspace
            .append_debug_log("updater", "Update check started");
    }

    /// Start downloading and applying an available update.
    pub(crate) fn trigger_update_apply(&mut self) {
        if !self.update_state.has_update() {
            return;
        }
        self.update_progress = UpdateProgress::Downloading;
        let (tx, rx) = std::sync::mpsc::channel();
        self.update_apply_rx = Some(rx);

        let state = self.update_state.clone();
        std::thread::spawn(move || {
            let result = updater::download_and_apply(&state);
            let _ = tx.send(result);
        });

        let _ = self
            .workspace
            .append_debug_log("updater", "Update download started");
    }

    /// Poll for update check/apply results.
    fn process_update_events(&mut self) {
        // Check for update check results
        if let Some(rx) = &self.update_check_rx {
            if let Ok(result) = rx.try_recv() {
                self.update_check_rx = None;
                match result {
                    Ok(state) => {
                        let has_update = state.has_update();
                        let version = state.available_version.clone();
                        self.update_state = state;
                        let state_path = updater::update_state_path(&self.workspace.root);
                        let _ = self.update_state.save(&state_path);

                        if has_update {
                            let ver = version.unwrap_or_default();
                            let _ = self.workspace.append_debug_log(
                                "updater",
                                &format!("Update available: v{}", ver),
                            );
                            self.update_progress = UpdateProgress::Available(ver);
                            self.update_banner_dismissed = false;
                        } else {
                            let _ = self
                                .workspace
                                .append_debug_log("updater", "No update available");
                            self.update_progress = UpdateProgress::Idle;
                        }
                    }
                    Err(e) => {
                        let _ = self
                            .workspace
                            .append_debug_log("updater", &format!("Update check failed: {}", e));
                        self.update_progress = UpdateProgress::Error(e);
                    }
                }
            }
        }

        // Check for update apply results
        if let Some(rx) = &self.update_apply_rx {
            if let Ok(result) = rx.try_recv() {
                self.update_apply_rx = None;
                match result {
                    Ok(update_result) => {
                        let _ = self.workspace.append_debug_log(
                            "updater",
                            &format!(
                                "Update applied: v{} (gui={}, cli={})",
                                update_result.version,
                                update_result.gui_updated,
                                update_result.cli_updated
                            ),
                        );
                        // Clear update state since we've applied it
                        self.update_state.clear_update();
                        let state_path = updater::update_state_path(&self.workspace.root);
                        let _ = self.update_state.save(&state_path);

                        self.update_progress =
                            UpdateProgress::ReadyToRestart(update_result.version);
                    }
                    Err(e) => {
                        let _ = self.workspace.append_debug_log(
                            "updater",
                            &format!("Update apply failed: {}", e),
                        );
                        self.update_progress = UpdateProgress::Error(e);
                    }
                }
            }
        }

        // Periodic auto-check (every 24 hours)
        if self.config.updates.auto_check
            && matches!(
                self.update_progress,
                UpdateProgress::Idle | UpdateProgress::Error(_)
            )
        {
            let should_check = match self.last_update_check {
                Some(last) => last.elapsed() >= Duration::from_secs(24 * 3600),
                None => self.update_state.needs_check(24),
            };
            if should_check {
                self.trigger_update_check();
            }
        }
    }

    fn process_tray_events(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.tray.check_event() {
            match event {
                TrayEvent::Open => {
                    let _ = self.workspace.append_debug_log(
                        "app",
                        "Processing tray open event; restoring viewport",
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::Quit => {
                    let _ = self
                        .workspace
                        .append_debug_log("app", "Processing tray quit event");
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
        self.process_watcher_events();
        self.process_update_events();

        // Another instance signaled us to come to the foreground
        if self.single_instance_guard.check_activation() {
            let _ = self
                .workspace
                .append_debug_log("app", "Another instance requested activation; restoring window");
            crate::tray::restore_window_native(&self.workspace.debug_log_path());
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // If quit was requested (e.g. from tray menu), initiate window close
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Handle close request - minimize instead of closing so tray events keep working.
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.should_quit {
                let _ = self
                    .workspace
                    .append_debug_log("app", "Close requested while quitting; allowing shutdown");
                // Let the close proceed by not cancelling it
            } else {
                let _ = self.workspace.append_debug_log(
                    "app",
                    "Close requested from window chrome; minimizing to tray",
                );
                // Hide the window so it disappears from the taskbar; the tray icon
                // listeners run on background threads and wake the event loop via
                // ctx.request_repaint(), so events keep flowing while hidden.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        // Auto-refresh task statuses and execution history every 2 seconds
        if self.last_refresh.elapsed().as_secs() >= 2 {
            self.refresh_task_statuses();

            if let View::TaskDetail(ref name) = self.current_view {
                self.selected_task_all_runs = self.workspace.load_runs_metadata(name, 200);
                let (page_runs, total) = Self::paginate_runs(
                    &self.selected_task_all_runs,
                    self.run_status_filter.as_ref(),
                    self.run_page,
                    self.runs_per_page,
                );
                self.selected_task_runs = page_runs;
                self.selected_task_runs_total = total;
            }
            self.last_refresh = Instant::now();
        }

        // Refresh live logs on a separate configurable timer
        let log_interval = Duration::from_secs_f32(self.log_refresh_interval_secs);
        if self.force_log_refresh || self.last_log_refresh.elapsed() >= log_interval {
            for (name, info) in &self.running_tasks {
                let content = self.workspace.read_output_log(name, &info.started_at);
                if !content.is_empty() {
                    self.live_logs.insert(name.clone(), content);
                }
            }
            self.force_log_refresh = false;
            self.last_log_refresh = Instant::now();
        }

        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        crate::ui::render(self, ctx);
    }
}

impl Drop for TaskPilotApp {
    fn drop(&mut self) {
        let _ = self
            .workspace
            .append_debug_log("app", "Application dropping; shutting down scheduler");
        let _ = self.scheduler.cmd_tx.send(SchedulerCommand::Shutdown);
    }
}
