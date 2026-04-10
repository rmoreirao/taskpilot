use crate::config::AppConfig;
use crate::executor::{execute_task, new_cancel_token, resolve_shell, CancelToken};
use crate::logging::LogLevel;
use crate::workspace::{TaskScheduleState, RunStatus, SchedulerState, Workspace};
use chrono::{DateTime, Local};
use cron::Schedule;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

/// How a task run was triggered.
#[derive(Debug, Clone)]
pub enum TaskTrigger {
    /// Normal on-time cron execution.
    Scheduled,
    /// Task was overdue — executed as a catch-up run.
    CatchUp {
        scheduled_for: DateTime<Local>,
    },
    /// User clicked "Run" in the UI or used `--run` CLI flag.
    Manual,
}

/// Threshold in seconds: if a task is overdue by more than this,
/// it's classified as a catch-up run rather than a normal scheduled run.
const CATCHUP_THRESHOLD_SECS: i64 = 60;

pub enum SchedulerCommand {
    RunTask(String),
    StopTask(String),
    UpdateConfig(AppConfig),
    Shutdown,
}

pub enum SchedulerEvent {
    TaskStarted(String, TaskTrigger),
    TaskFinished(String, RunStatus, TaskTrigger),
    TaskSkipped {
        name: String,
        scheduled_for: DateTime<Local>,
        reason: String,
    },
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
    let running: Arc<Mutex<HashMap<String, CancelToken>>> = Arc::new(Mutex::new(HashMap::new()));

    // Initialize / reconcile state for all configured tasks
    for task in &config.tasks {
        if let Some(schedule) = parse_cron(&task.cron) {
            if let Some(existing) = state.tasks.get_mut(&task.name) {
                // Check if cron expression changed since last persist
                let cron_changed = existing.cron_expr.as_deref() != Some(&task.cron);
                if cron_changed {
                    workspace.log_task(
                        LogLevel::Info,
                        "scheduler",
                        &format!(
                            "Task '{}': cron changed from '{}' to '{}' — recomputing next_run",
                            task.name,
                            existing.cron_expr.as_deref().unwrap_or("unknown"),
                            task.cron
                        ),
                    );
                    existing.next_run = schedule.upcoming(Local).next();
                    existing.cron_expr = Some(task.cron.clone());
                }
            } else {
                // New task — initialize state
                let next = schedule.upcoming(Local).next();
                workspace.log_task(
                    LogLevel::Debug,
                    "scheduler",
                    &format!(
                        "Task '{}': cron '{}' → next run at {}",
                        task.name,
                        task.cron,
                        next.map_or("none".to_string(), |t| t.to_rfc3339())
                    ),
                );
                state.tasks.insert(
                    task.name.clone(),
                    TaskScheduleState {
                        last_run: None,
                        next_run: next,
                        last_status: None,
                        cron_expr: Some(task.cron.clone()),
                    },
                );
            }
        }
    }

    // Persist reconciled state before entering tick loop
    let _ = workspace.save_state(&state);

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
                        TaskTrigger::Manual,
                    );
                }
                Ok(SchedulerCommand::StopTask(name)) => {
                    let token = running.lock().unwrap().get(&name).cloned();
                    if let Some(cancel) = token {
                        workspace.log_task(
                            LogLevel::Info,
                            "scheduler",
                            &format!("Stop requested for task '{}'", name),
                        );
                        cancel.store(true, Ordering::Relaxed);
                    } else {
                        workspace.log_task(
                            LogLevel::Debug,
                            "scheduler",
                            &format!("Stop requested for task '{}' but it is not running", name),
                        );
                    }
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
                                        cron_expr: None,
                                    });
                            // Only reset next_run if cron expression changed
                            let cron_changed = entry.cron_expr.as_deref() != Some(&task.cron);
                            if cron_changed || entry.next_run.is_none() {
                                entry.next_run = schedule.upcoming(Local).next();
                                entry.cron_expr = Some(task.cron.clone());
                            }
                            workspace.log_task(
                                LogLevel::Debug,
                                "scheduler",
                                &format!(
                                    "Task '{}': cron '{}' → next run at {}",
                                    task.name,
                                    task.cron,
                                    entry
                                        .next_run
                                        .map_or("none".to_string(), |t| t.to_rfc3339())
                                ),
                            );
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // Check cron schedules — classify overdue vs on-time
        let now = Local::now();

        // Collect tasks that are due, along with their trigger classification
        let mut tasks_to_run: Vec<(String, TaskTrigger)> = Vec::new();
        let mut tasks_to_skip: Vec<(String, DateTime<Local>)> = Vec::new();

        for task in &config.tasks {
            let is_running = running.lock().unwrap().contains_key(&task.name);
            if is_running {
                continue;
            }
            let next_run = match state.tasks.get(&task.name).and_then(|s| s.next_run) {
                Some(next) if now >= next => next,
                _ => continue,
            };

            let late_by = (now - next_run).num_seconds();

            if late_by > CATCHUP_THRESHOLD_SECS {
                // This is a missed/overdue run
                if task.run_missed {
                    workspace.log_task(
                        LogLevel::Info,
                        "scheduler",
                        &format!(
                            "Task '{}': catch-up run (was due at {}, {}s late)",
                            task.name,
                            next_run.to_rfc3339(),
                            late_by
                        ),
                    );
                    tasks_to_run.push((
                        task.name.clone(),
                        TaskTrigger::CatchUp {
                            scheduled_for: next_run,
                        },
                    ));
                } else {
                    workspace.log_task(
                        LogLevel::Info,
                        "scheduler",
                        &format!(
                            "Task '{}': skipping overdue run (was due at {}, {}s late, run_missed=false)",
                            task.name,
                            next_run.to_rfc3339(),
                            late_by
                        ),
                    );
                    tasks_to_skip.push((task.name.clone(), next_run));
                }
            } else {
                // Normal on-time run
                workspace.log_task(
                    LogLevel::Info,
                    "scheduler",
                    &format!(
                        "Task '{}' triggered by cron schedule (next_run was {})",
                        task.name,
                        next_run.to_rfc3339(),
                    ),
                );
                tasks_to_run.push((task.name.clone(), TaskTrigger::Scheduled));
            }
        }

        // Handle skipped tasks: advance next_run and notify
        for (name, scheduled_for) in tasks_to_skip {
            if let Some(task_cfg) = config.tasks.iter().find(|t| t.name == name) {
                if let Some(schedule) = parse_cron(&task_cfg.cron) {
                    if let Some(task_state) = state.tasks.get_mut(&name) {
                        task_state.next_run = schedule.upcoming(Local).next();
                        task_state.cron_expr = Some(task_cfg.cron.clone());
                    }
                }
            }
            let _ = workspace.save_state(&state);
            let _ = evt_tx.send(SchedulerEvent::TaskSkipped {
                name,
                scheduled_for,
                reason: "run_missed=false".to_string(),
            });
        }

        // Run due tasks
        for (name, trigger) in tasks_to_run {
            spawn_task(&config, &name, &workspace, &evt_tx, &running, &mut state, trigger);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn spawn_task(
    config: &AppConfig,
    name: &str,
    workspace: &Arc<Workspace>,
    evt_tx: &mpsc::Sender<SchedulerEvent>,
    running: &Arc<Mutex<HashMap<String, CancelToken>>>,
    state: &mut SchedulerState,
    trigger: TaskTrigger,
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

    let cancel = new_cancel_token();

    {
        let mut r = running.lock().unwrap();
        if r.contains_key(name) {
            workspace.log_task(
                LogLevel::Debug,
                "scheduler",
                &format!("Task '{}' skipped: already running", name),
            );
            return;
        }
        r.insert(name.to_string(), cancel.clone());
    }

    // Update state before execution
    if let Some(task_state) = state.tasks.get_mut(name) {
        task_state.last_run = Some(Local::now());
        if let Some(schedule) = parse_cron(&task.cron) {
            task_state.next_run = schedule.upcoming(Local).next();
            task_state.cron_expr = Some(task.cron.clone());
            workspace.log_task(
                LogLevel::Debug,
                "scheduler",
                &format!("Task '{}': next run at {}", name,
                    task_state.next_run.map_or("none".to_string(), |t| t.to_rfc3339())),
            );
        }
    }
    let _ = workspace.save_state(state);

    let _ = evt_tx.send(SchedulerEvent::TaskStarted(name.to_string(), trigger.clone()));

    let ws = Arc::clone(workspace);
    let evt = evt_tx.clone();
    let running_set = Arc::clone(running);
    let task_name = name.to_string();
    let notify_cfg = config.notifications.clone();
    let task_cron = task.cron.clone();
    let shell = resolve_shell(task.shell, config.general.default_shell);

    thread::spawn(move || {
        let run = execute_task(&task, &ws, &cancel, shell);
        let status = run.status.clone();

        // Persist last_status and re-advance next_run to first future occurrence
        {
            let mut updated_state = ws.load_state();
            if let Some(task_state) = updated_state.tasks.get_mut(&task_name) {
                task_state.last_status = Some(status.clone());
                // Re-advance next_run to ensure it's always in the future
                if let Some(schedule) = parse_cron(&task_cron) {
                    let next = schedule.upcoming(Local).next();
                    if task_state.next_run.map_or(true, |nr| nr <= Local::now()) {
                        task_state.next_run = next;
                    }
                }
            }
            let _ = ws.save_state(&updated_state);
        }

        // OS notification on failure (suppress for user-initiated stops)
        if (status == RunStatus::Failed || status == RunStatus::Timeout)
            && notify_cfg.enabled
            && task.notify_on_failure
        {
            send_failure_notification(&task.name, &run.stderr);
        }

        // Remove from running map before sending event to avoid race
        running_set.lock().unwrap().remove(&task_name);
        let _ = evt.send(SchedulerEvent::TaskFinished(task_name.clone(), status, trigger));
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
