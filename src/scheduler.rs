use crate::config::AppConfig;
use crate::executor::execute_job;
use crate::workspace::{JobScheduleState, RunStatus, SchedulerState, Workspace};
use chrono::Utc;
use cron::Schedule;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum SchedulerCommand {
    RunJob(String),
    UpdateConfig(AppConfig),
    Shutdown,
}

pub enum SchedulerEvent {
    JobStarted(String),
    JobFinished(String, RunStatus),
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

    // Initialize next_run for all jobs
    for job in &config.jobs {
        if !state.jobs.contains_key(&job.name) {
            if let Some(schedule) = parse_cron(&job.cron) {
                state.jobs.insert(
                    job.name.clone(),
                    JobScheduleState {
                        last_run: None,
                        next_run: schedule.upcoming(Utc).next(),
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
                Ok(SchedulerCommand::Shutdown) => return,
                Ok(SchedulerCommand::RunJob(name)) => {
                    spawn_job(
                        &config,
                        &name,
                        &workspace,
                        &evt_tx,
                        &running,
                        &mut state,
                    );
                }
                Ok(SchedulerCommand::UpdateConfig(new_config)) => {
                    config = new_config;
                    for job in &config.jobs {
                        if let Some(schedule) = parse_cron(&job.cron) {
                            let entry =
                                state
                                    .jobs
                                    .entry(job.name.clone())
                                    .or_insert(JobScheduleState {
                                        last_run: None,
                                        next_run: None,
                                        last_status: None,
                                    });
                            entry.next_run = schedule.upcoming(Utc).next();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // Check cron schedules
        let now = Utc::now();
        let jobs_to_run: Vec<String> = config
            .jobs
            .iter()
            .filter(|job| {
                let is_running = running.lock().unwrap().contains(&job.name);
                if is_running {
                    return false;
                }
                state
                    .jobs
                    .get(&job.name)
                    .and_then(|s| s.next_run)
                    .map_or(false, |next| now >= next)
            })
            .map(|j| j.name.clone())
            .collect();

        for name in jobs_to_run {
            spawn_job(&config, &name, &workspace, &evt_tx, &running, &mut state);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn spawn_job(
    config: &AppConfig,
    name: &str,
    workspace: &Arc<Workspace>,
    evt_tx: &mpsc::Sender<SchedulerEvent>,
    running: &Arc<Mutex<HashSet<String>>>,
    state: &mut SchedulerState,
) {
    let job = match config.jobs.iter().find(|j| j.name == name) {
        Some(j) => j.clone(),
        None => return,
    };

    {
        let mut r = running.lock().unwrap();
        if r.contains(name) {
            return;
        }
        r.insert(name.to_string());
    }

    // Update state
    if let Some(job_state) = state.jobs.get_mut(name) {
        job_state.last_run = Some(Utc::now());
        if let Some(schedule) = parse_cron(&job.cron) {
            job_state.next_run = schedule.upcoming(Utc).next();
        }
    }
    let _ = workspace.save_state(state);

    let _ = evt_tx.send(SchedulerEvent::JobStarted(name.to_string()));

    let ws = Arc::clone(workspace);
    let evt = evt_tx.clone();
    let running_set = Arc::clone(running);
    let job_name = name.to_string();
    let notify_cfg = config.notifications.clone();

    thread::spawn(move || {
        let run = execute_job(&job, &ws);
        let status = run.status.clone();

        // OS notification on failure
        if (status == RunStatus::Failed || status == RunStatus::Timeout)
            && notify_cfg.enabled
            && job.notify_on_failure
        {
            send_failure_notification(&job.name, &run.stderr);
        }

        let _ = evt.send(SchedulerEvent::JobFinished(job_name.clone(), status));
        running_set.lock().unwrap().remove(&job_name);
    });
}

fn send_failure_notification(job_name: &str, error: &str) {
    let error_preview: String = error.chars().take(200).collect();
    let _ = notify_rust::Notification::new()
        .summary(&format!("TaskPilot: {} failed", job_name))
        .body(&error_preview)
        .timeout(notify_rust::Timeout::Milliseconds(10000))
        .show();
}
