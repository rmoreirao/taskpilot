use crate::config::AppConfig;
use crate::scheduler::{SchedulerCommand, SchedulerEvent, SchedulerHandle, start_scheduler};
use crate::task_sources::{self, TaskSourceInfo};
use crate::tray::{TrayEvent, TrayManager};
use crate::workspace::{TaskRun, RunStatus, Workspace};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Tasks,
    TaskDetail(String),
    Settings,
    Notifications,
}

#[derive(Clone)]
pub struct TaskStatus {
    pub name: String,
    pub command: String,
    pub cron: String,
    pub last_run: Option<TaskRun>,
    pub is_running: bool,
}

#[derive(Clone)]
pub struct NotificationItem {
    pub task_name: String,
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
    pub(crate) task_statuses: Vec<TaskStatus>,
    pub(crate) running_tasks: HashSet<String>,
    pub(crate) selected_task_runs: Vec<TaskRun>,
    last_refresh: Instant,
    pub(crate) notifications: Vec<NotificationItem>,
    pub(crate) search_filter: String,
    tray: TrayManager,
    should_quit: bool,
    pub(crate) source_metadata: HashMap<String, TaskSourceInfo>,
    pub(crate) source_dirs: Vec<PathBuf>,
    cli_task_dirs: Vec<PathBuf>,
    _watcher: Option<notify::RecommendedWatcher>,
    watcher_rx: Option<std::sync::mpsc::Receiver<()>>,
}

impl TaskPilotApp {
    pub fn new(
        workspace: Arc<Workspace>,
        config: AppConfig,
        tray: TrayManager,
        source_metadata: HashMap<String, TaskSourceInfo>,
        source_dirs: Vec<PathBuf>,
        cli_task_dirs: Vec<PathBuf>,
    ) -> Self {
        let config_content = workspace.config_content();
        let scheduler = start_scheduler(config.clone(), Arc::clone(&workspace));

        // Set up file watcher for external source directories
        let (watcher, watcher_rx) = Self::create_watcher(&source_dirs);

        let mut app = Self {
            workspace,
            config,
            config_content,
            scheduler,
            current_view: View::Tasks,
            task_statuses: Vec::new(),
            running_tasks: HashSet::new(),
            selected_task_runs: Vec::new(),
            last_refresh: Instant::now(),
            notifications: Vec::new(),
            search_filter: String::new(),
            tray,
            should_quit: false,
            source_metadata,
            source_dirs,
            cli_task_dirs,
            _watcher: watcher,
            watcher_rx,
        };
        app.refresh_task_statuses();
        let _ = app
            .workspace
            .append_debug_log("app", "TaskPilot application initialized");
        app
    }

    fn create_watcher(
        dirs: &[PathBuf],
    ) -> (
        Option<notify::RecommendedWatcher>,
        Option<std::sync::mpsc::Receiver<()>>,
    ) {
        use notify::{RecursiveMode, Watcher};

        if dirs.is_empty() {
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
                (Some(watcher), Some(rx))
            }
            Err(e) => {
                eprintln!("Failed to create file watcher: {}", e);
                (None, None)
            }
        }
    }

    pub(crate) fn refresh_task_statuses(&mut self) {
        self.task_statuses = self
            .config
            .tasks
            .iter()
            .map(|task| {
                let last_run = self.workspace.get_latest_run(&task.name);
                TaskStatus {
                    name: task.name.clone(),
                    command: task.command.clone(),
                    cron: task.cron.clone(),
                    last_run,
                    is_running: self.running_tasks.contains(&task.name),
                }
            })
            .collect();
    }

    fn process_events(&mut self) {
        while let Ok(evt) = self.scheduler.evt_rx.try_recv() {
            match evt {
                SchedulerEvent::TaskStarted(name) => {
                    self.running_tasks.insert(name);
                }
                SchedulerEvent::TaskFinished(name, status) => {
                    self.running_tasks.remove(&name);
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
                            task_name: name,
                            status,
                            time: chrono::Utc::now(),
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
        match AppConfig::load(&self.workspace.config_path()) {
            Ok(new_config) => {
                // Rebuild source dirs: config task_sources + CLI --task-dir
                let mut source_dirs: Vec<PathBuf> = new_config
                    .general
                    .task_sources
                    .iter()
                    .map(PathBuf::from)
                    .collect();
                for dir in &self.cli_task_dirs {
                    if !source_dirs.contains(dir) {
                        source_dirs.push(dir.clone());
                    }
                }

                // Reload external tasks
                match task_sources::load_all(
                    &new_config.tasks,
                    &self.workspace.config_path(),
                    &source_dirs,
                ) {
                    Ok((merged_tasks, source_metadata)) => {
                        let mut effective_config = new_config;
                        effective_config.tasks = merged_tasks;

                        self.config = effective_config.clone();
                        self.config_content = self.workspace.config_content();
                        self.source_metadata = source_metadata;

                        // Recreate watcher if source dirs changed
                        if source_dirs != self.source_dirs {
                            let (watcher, watcher_rx) = Self::create_watcher(&source_dirs);
                            self._watcher = watcher;
                            self.watcher_rx = watcher_rx;
                            self.source_dirs = source_dirs;
                        }

                        let _ = self
                            .scheduler
                            .cmd_tx
                            .send(SchedulerCommand::UpdateConfig(effective_config));
                        let _ = self
                            .workspace
                            .append_debug_log("app", "Config reloaded successfully with external sources");
                    }
                    Err(e) => {
                        eprintln!("Failed to load external task sources: {}", e);
                        let _ = self
                            .workspace
                            .append_debug_log("app", &format!("External source error: {}", e));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to reload config: {}", e);
            }
        }
    }

    pub fn quit_app(&mut self) {
        let _ = self
            .workspace
            .append_debug_log("app", "Quit requested; setting should_quit");
        self.should_quit = true;
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
                // Actually quit the app
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                let _ = self.workspace.append_debug_log(
                    "app",
                    "Close requested from window chrome; minimizing to tray",
                );
                // Minimize instead of hiding so the native event loop keeps pumping tray events.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        // Auto-refresh from disk every 2 seconds
        if self.last_refresh.elapsed().as_secs() >= 2 {
            self.refresh_task_statuses();
            if let View::TaskDetail(ref name) = self.current_view {
                self.selected_task_runs = self.workspace.load_runs(name, 50);
            }
            self.last_refresh = Instant::now();
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
