use crate::config::AppConfig;
use crate::executor::execute_task;
use crate::logging::LogLevel;
use crate::workspace::{TaskScheduleState, RunStatus, SchedulerState, Workspace};
use chrono::Utc;
use cron::Schedule;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum SchedulerCommand {
    RunTask(String),
    UpdateConfig(AppConfig),
    Shutdown,
}

pub enum SchedulerEvent {
    TaskStarted(String),
    TaskFinished(String, RunStatus),
}

pub struct SchedulerHandle {
    pub cmd_tx: mpsc::Sender<SchedulerCommand>,
    pub evt_rx: mpsc::Receiver<SchedulerEvent>,
}

pub fn start_scheduler(config: AppConfig, workspace: Arc<Workspace>) -> SchedulerHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();

    thread::spawn(move || {
        scheduler_loop(config, workspace, cmd_rx, evt_tx);
    });

    SchedulerHandle { cmd_tx, evt_rx }
}

fn parse_cron(expr: &str) -> Option<Schedule> {
    // Standard 5-field cron → prepend seconds field "0"
    let fields = expr.split_whitespace().count();
    let full_expr = if fields == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    };
    Schedule::from_str(&full_expr).ok()
}

fn scheduler_loop(
    mut config: AppConfig,
    workspace: Arc<Workspace>,
    cmd_rx: mpsc::Receiver<SchedulerCommand>,
    evt_tx: mpsc::Sender<SchedulerEvent>,
) {
    let mut state = workspace.load_state();
    let running: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    // Initialize next_run for all tasks
    for task in &config.tasks {
        if !state.tasks.contains_key(&task.name) {
            if let Some(schedule) = parse_cron(&task.cron) {
                let next = schedule.upcoming(Utc).next();
                workspace.log_task(
                    LogLevel::Debug,
                    "scheduler",
                    &format!("Task '{}': cron '{}' → next run at {}", task.name, task.cron,
                        next.map_or("none".to_string(), |t| t.to_rfc3339())),
                );
                state.tasks.insert(
                    task.name.clone(),
                    TaskScheduleState {
                        last_run: None,
                        next_run: next,
                        last_status: None,
                    },
                );
            }
        }
    }

    loop {
        // Process pending commands
        loop {
            match cmd_rx.try_recv() {
                Ok(SchedulerCommand::Shutdown) => {
                    workspace.log_task(LogLevel::Info, "scheduler", "Scheduler shutting down");
                    return;
                }
                Ok(SchedulerCommand::RunTask(name)) => {
                    workspace.log_task(
                        LogLevel::Info,
                        "scheduler",
                        &format!("Manual run requested for task '{}'", name),
                    );
                    spawn_task(
                        &config,
                        &name,
                        &workspace,
                        &evt_tx,
                        &running,
                        &mut state,
                    );
                }
                Ok(SchedulerCommand::UpdateConfig(new_config)) => {
                    workspace.log_task(
                        LogLevel::Info,
                        "scheduler",
                        &format!("Config updated: rescheduling {} tasks", new_config.tasks.len()),
                    );
                    config = new_config;
                    for task in &config.tasks {
                        if let Some(schedule) = parse_cron(&task.cron) {
                            let entry =
                                state
                                    .tasks
                                    .entry(task.name.clone())
                                    .or_insert(TaskScheduleState {
                                        last_run: None,
                                        next_run: None,
                                        last_status: None,
                                    });
                            entry.next_run = schedule.upcoming(Utc).next();
                            workspace.log_task(
                                LogLevel::Debug,
                                "scheduler",
                                &format!("Task '{}': cron '{}' → next run at {}", task.name, task.cron,
                                    entry.next_run.map_or("none".to_string(), |t| t.to_rfc3339())),
                            );
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // Check cron schedules
        let now = Utc::now();
        let tasks_to_run: Vec<String> = config
            .tasks
            .iter()
            .filter(|task| {
                let is_running = running.lock().unwrap().contains(&task.name);
                if is_running {
                    return false;
                }
                state
                    .tasks
                    .get(&task.name)
                    .and_then(|s| s.next_run)
                    .map_or(false, |next| now >= next)
            })
            .map(|t| t.name.clone())
            .collect();

        for name in &tasks_to_run {
            workspace.log_task(
                LogLevel::Info,
                "scheduler",
                &format!(
                    "Task '{}' triggered by cron schedule (next_run was {})",
                    name,
                    state.tasks.get(name)
                        .and_then(|s| s.next_run)
                        .map_or("none".to_string(), |t| t.to_rfc3339()),
                ),
            );
        }

        for name in tasks_to_run {
            spawn_task(&config, &name, &workspace, &evt_tx, &running, &mut state);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn spawn_task(
    config: &AppConfig,
    name: &str,
    workspace: &Arc<Workspace>,
    evt_tx: &mpsc::Sender<SchedulerEvent>,
    running: &Arc<Mutex<HashSet<String>>>,
    state: &mut SchedulerState,
) {
    let task = match config.tasks.iter().find(|t| t.name == name) {
        Some(t) => t.clone(),
        None => {
            workspace.log_task(
                LogLevel::Warn,
                "scheduler",
                &format!("Task '{}' not found in config (run request ignored)", name),
            );
            return;
        }
    };

    {
        let mut r = running.lock().unwrap();
        if r.contains(name) {
            workspace.log_task(
                LogLevel::Debug,
                "scheduler",
                &format!("Task '{}' skipped: already running", name),
            );
            return;
        }
        r.insert(name.to_string());
    }

    // Update state
    if let Some(task_state) = state.tasks.get_mut(name) {
        task_state.last_run = Some(Utc::now());
        if let Some(schedule) = parse_cron(&task.cron) {
            task_state.next_run = schedule.upcoming(Utc).next();
            workspace.log_task(
                LogLevel::Debug,
                "scheduler",
                &format!("Task '{}': next run at {}", name,
                    task_state.next_run.map_or("none".to_string(), |t| t.to_rfc3339())),
            );
        }
    }
    let _ = workspace.save_state(state);

    let _ = evt_tx.send(SchedulerEvent::TaskStarted(name.to_string()));

    let ws = Arc::clone(workspace);
    let evt = evt_tx.clone();
    let running_set = Arc::clone(running);
    let task_name = name.to_string();
    let notify_cfg = config.notifications.clone();

    thread::spawn(move || {
        let run = execute_task(&task, &ws);
        let status = run.status.clone();

        // OS notification on failure
        if (status == RunStatus::Failed || status == RunStatus::Timeout)
            && notify_cfg.enabled
            && task.notify_on_failure
        {
            send_failure_notification(&task.name, &run.stderr);
        }

        let _ = evt.send(SchedulerEvent::TaskFinished(task_name.clone(), status));
        running_set.lock().unwrap().remove(&task_name);
    });
}

fn send_failure_notification(task_name: &str, error: &str) {
    let error_preview: String = error.chars().take(200).collect();
    let _ = notify_rust::Notification::new()
        .summary(&format!("TaskPilot: {} failed", task_name))
        .body(&error_preview)
        .timeout(notify_rust::Timeout::Milliseconds(10000))
        .show();
}
