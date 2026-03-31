use crate::config::TaskConfig;
use crate::workspace::{TaskRun, RunStatus, Workspace};
use chrono::Utc;
use std::io::Read;
use std::process::Command;
use std::time::{Duration, Instant};

pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse::<u64>().ok().map(Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<u64>().ok().map(|m| Duration::from_secs(m * 60))
    } else if let Some(hrs) = s.strip_suffix('h') {
        hrs.parse::<u64>().ok().map(|h| Duration::from_secs(h * 3600))
    } else {
        s.parse::<u64>().ok().map(Duration::from_secs)
    }
}

pub fn execute_task(task: &TaskConfig, workspace: &Workspace) -> TaskRun {
    let started_at = Utc::now();
    let start_instant = Instant::now();
    let timeout = task.timeout.as_ref().and_then(|t| parse_duration(t));
    let retries = task.retries.unwrap_or(0);

    let mut last_run = None;

    for attempt in 0..=retries {
        let result = run_command(&task.command, task.working_dir.as_deref(), timeout);
        let elapsed = start_instant.elapsed();
        let finished_at = Utc::now();

        let run = TaskRun {
            task_name: task.name.clone(),
            status: result.status,
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(elapsed.as_millis() as u64),
        };

        if run.status == RunStatus::Passed || attempt == retries {
            let _ = workspace.save_run(&run);
            last_run = Some(run);
            break;
        }

        last_run = Some(run);
    }

    last_run.unwrap()
}

struct CommandResult {
    status: RunStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_command(
    command: &str,
    working_dir: Option<&str>,
    timeout: Option<Duration>,
) -> CommandResult {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };

    if let Some(dir) = working_dir {
        let expanded = expand_home(dir);
        cmd.current_dir(&expanded);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return CommandResult {
                status: RunStatus::Failed,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to spawn process: {}", e),
            };
        }
    };

    if let Some(timeout_dur) = timeout {
        match wait_with_timeout(child, timeout_dur) {
            Ok(output) => {
                let exit_code = output.status.code();
                let status = if output.status.success() {
                    RunStatus::Passed
                } else {
                    RunStatus::Failed
                };
                CommandResult {
                    status,
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                }
            }
            Err(_) => CommandResult {
                status: RunStatus::Timeout,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Process timed out after {:?}", timeout_dur),
            },
        }
    } else {
        match child.wait_with_output() {
            Ok(output) => {
                let exit_code = output.status.code();
                let status = if output.status.success() {
                    RunStatus::Passed
                } else {
                    RunStatus::Failed
                };
                CommandResult {
                    status,
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                }
            }
            Err(e) => CommandResult {
                status: RunStatus::Failed,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to wait for process: {}", e),
            },
        }
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, ()> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf);
                        buf
                    })
                    .unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Err(()),
        }
    }
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}
