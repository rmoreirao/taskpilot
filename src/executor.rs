use crate::config::{Shell, TaskConfig};
use crate::logging::LogLevel;
use crate::workspace::{TaskRun, TaskRunConfig, RunStatus, Workspace};
use chrono::{DateTime, Local};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub type CancelToken = Arc<AtomicBool>;

pub fn new_cancel_token() -> CancelToken {
    Arc::new(AtomicBool::new(false))
}

enum WaitError {
    Timeout,
    Cancelled,
    Other,
}

/// Resolved shell configuration used to spawn a process.
struct ShellSpec {
    program: &'static str,
    /// Arguments inserted before the user command (e.g. `-NoProfile`, `-NonInteractive`, `-Command`).
    pre_args: Vec<&'static str>,
    /// When true, the command string is passed via `raw_arg` (needed for cmd.exe quoting).
    uses_raw_arg: bool,
}

impl ShellSpec {
    fn from_shell(shell: Shell) -> Self {
        match shell {
            Shell::Cmd => ShellSpec {
                program: "cmd",
                pre_args: vec!["/C"],
                uses_raw_arg: true,
            },
            Shell::PowerShell => ShellSpec {
                program: "powershell.exe",
                pre_args: vec!["-NoProfile", "-NonInteractive", "-Command"],
                uses_raw_arg: false,
            },
            Shell::Pwsh => ShellSpec {
                program: "pwsh.exe",
                pre_args: vec!["-NoProfile", "-NonInteractive", "-Command"],
                uses_raw_arg: false,
            },
            Shell::Sh => ShellSpec {
                program: "sh",
                pre_args: vec!["-c"],
                uses_raw_arg: false,
            },
            Shell::Bash => ShellSpec {
                program: "bash",
                pre_args: vec!["-c"],
                uses_raw_arg: false,
            },
        }
    }
}

/// Resolve the effective shell: task-level > global default > platform default.
pub fn resolve_shell(task_shell: Option<Shell>, default_shell: Option<Shell>) -> Shell {
    task_shell
        .or(default_shell)
        .unwrap_or_else(Shell::platform_default)
}

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

pub fn execute_task(task: &TaskConfig, workspace: &Workspace, cancel: &CancelToken, shell: Shell) -> TaskRun {
    execute_task_at(task, workspace, cancel, shell, Local::now())
}

pub fn execute_task_at(task: &TaskConfig, workspace: &Workspace, cancel: &CancelToken, shell: Shell, started_at: DateTime<Local>) -> TaskRun {
    let start_instant = Instant::now();
    let timeout = task.timeout.as_ref().and_then(|t| parse_duration(t));
    let retries = task.retries.unwrap_or(0);

    let run_config = TaskRunConfig {
        command: task.command.clone(),
        cron: task.cron.clone().unwrap_or_default(),
        timeout: task.timeout.clone(),
        working_dir: task.working_dir.clone(),
        notify_on_failure: task.notify_on_failure,
        retries,
        run_missed: task.run_missed,
        shell: format!("{}", shell),
    };

    workspace.log_task(
        LogLevel::Info,
        "executor",
        &format!("Starting task '{}': command=\"{}\"", task.name, task.command),
    );
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!(
            "Task '{}' config: cron=\"{}\", timeout={}, working_dir={}, retries={}, notify_on_failure={}, shell={}",
            task.name,
            task.cron.as_deref().unwrap_or("none"),
            task.timeout.as_deref().unwrap_or("none"),
            task.working_dir.as_deref().unwrap_or("(default)"),
            retries,
            task.notify_on_failure,
            shell,
        ),
    );
    if let Some(ref dur) = timeout {
        workspace.log_task(
            LogLevel::Debug,
            "executor",
            &format!("Task '{}': parsed timeout = {:?}", task.name, dur),
        );
    }

    // Create the run directory and output.log path
    let run_dir = workspace.run_dir(&task.name, &started_at);
    let _ = std::fs::create_dir_all(&run_dir);
    let output_log_path = run_dir.join("output.log");
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': output log path = \"{}\"", task.name, output_log_path.display()),
    );

    let mut last_run = None;
    let total_attempts = retries + 1;

    for attempt in 0..=retries {
        // Check cancellation before each attempt
        if cancel.load(Ordering::Relaxed) {
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!("Task '{}': cancelled before attempt {}/{}", task.name, attempt + 1, total_attempts),
            );
            let elapsed = start_instant.elapsed();
            let run = TaskRun {
                task_name: task.name.clone(),
                status: RunStatus::Stopped,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                started_at,
                finished_at: Some(Local::now()),
                duration_ms: Some(elapsed.as_millis() as u64),
                attempt: Some(attempt),
                total_attempts: Some(total_attempts),
                config: Some(run_config),
                output_log_path: Some(output_log_path),
            };
            let _ = workspace.save_run(&run);
            return run;
        }

        if attempt > 0 {
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!("Task '{}': retry attempt {}/{} (attempt {}/{})", task.name, attempt, retries, attempt + 1, total_attempts),
            );
            // Append a separator to the output log between attempts
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&output_log_path) {
                let _ = writeln!(f, "\n--- Retry attempt {}/{} ---\n", attempt + 1, total_attempts);
            }
        }

        let result = run_command(
            &task.command,
            task.working_dir.as_deref(),
            timeout,
            &output_log_path,
            workspace,
            &task.name,
            cancel,
            shell,
        );
        let elapsed = start_instant.elapsed();
        let finished_at = Local::now();

        let run = TaskRun {
            task_name: task.name.clone(),
            status: result.status,
            exit_code: result.exit_code,
            stdout: String::new(),
            stderr: String::new(),
            started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(elapsed.as_millis() as u64),
            attempt: Some(attempt),
            total_attempts: Some(total_attempts),
            config: Some(run_config.clone()),
            output_log_path: Some(output_log_path.clone()),
        };

        // Stop retrying on success, cancellation, or final attempt
        if run.status == RunStatus::Passed
            || run.status == RunStatus::Stopped
            || attempt == retries
        {
            if attempt > 0 && run.status != RunStatus::Passed {
                workspace.log_task(
                    LogLevel::Warn,
                    "executor",
                    &format!(
                        "Task '{}': all {} retries exhausted, final status={}",
                        task.name,
                        retries,
                        match run.status {
                            RunStatus::Failed => "failed",
                            RunStatus::Timeout => "timeout",
                            RunStatus::Stopped => "stopped",
                            _ => "unknown",
                        },
                    ),
                );
            }
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!(
                    "Task '{}': completed attempt={}/{} status={} exit_code={} duration={}ms",
                    task.name,
                    attempt + 1,
                    total_attempts,
                    match run.status {
                        RunStatus::Passed => "passed",
                        RunStatus::Failed => "failed",
                        RunStatus::Timeout => "timeout",
                        RunStatus::Running => "running",
                        RunStatus::Stopped => "stopped",
                    },
                    run.exit_code.map_or("none".to_string(), |c| c.to_string()),
                    elapsed.as_millis(),
                ),
            );
            let _ = workspace.save_run(&run);
            last_run = Some(run);
            break;
        }

        // Log the failed attempt before retrying
        workspace.log_task(
            LogLevel::Warn,
            "executor",
            &format!(
                "Task '{}': attempt {}/{} failed (status={}, exit_code={}), will retry",
                task.name,
                attempt + 1,
                total_attempts,
                match run.status {
                    RunStatus::Failed => "failed",
                    RunStatus::Timeout => "timeout",
                    _ => "unknown",
                },
                run.exit_code.map_or("none".to_string(), |c| c.to_string()),
            ),
        );

        last_run = Some(run);
    }

    last_run.unwrap()
}

struct CommandResult {
    status: RunStatus,
    exit_code: Option<i32>,
}

/// Shared writer for the live log file, used by stdout/stderr reader threads.
type LiveLogWriter = Arc<Mutex<std::io::BufWriter<std::fs::File>>>;

/// Create a log writer for the given path (append mode). Returns None if the file can't be opened.
fn open_live_log(path: &Path) -> Option<LiveLogWriter> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(|f| Arc::new(Mutex::new(std::io::BufWriter::new(f))))
}

/// Spawn a thread that reads lines from `reader` and appends them to the log file.
/// No in-memory buffering — output goes directly to disk to avoid OOM on chatty tasks.
fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    prefix: &str,
    log_writer: Option<LiveLogWriter>,
) -> std::thread::JoinHandle<()> {
    let prefix = prefix.to_string();

    std::thread::spawn(move || {
        let mut br = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match br.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Some(ref writer) = log_writer {
                        if let Ok(mut w) = writer.lock() {
                            let _ = write!(w, "[{}] {}", prefix, line);
                            let _ = w.flush();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn run_command(
    command: &str,
    working_dir: Option<&str>,
    timeout: Option<Duration>,
    output_log_path: &Path,
    workspace: &Workspace,
    task_name: &str,
    cancel: &CancelToken,
    shell: Shell,
) -> CommandResult {
    let spec = ShellSpec::from_shell(shell);
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': shell={} command=\"{}\"", task_name, shell, command),
    );

    let mut cmd = Command::new(spec.program);

    #[cfg(windows)]
    {
        if spec.uses_raw_arg {
            for arg in &spec.pre_args {
                cmd.raw_arg(arg);
            }
            cmd.raw_arg(command);
        } else {
            for arg in &spec.pre_args {
                cmd.arg(arg);
            }
            cmd.arg(command);
        }
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    #[cfg(not(windows))]
    {
        for arg in &spec.pre_args {
            cmd.arg(arg);
        }
        cmd.arg(command);
    }

    if let Some(dir) = working_dir {
        let expanded = expand_home(dir);
        workspace.log_task(
            LogLevel::Debug,
            "executor",
            &format!("Task '{}': resolved working_dir = \"{}\"", task_name, expanded),
        );
        cmd.current_dir(&expanded);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            workspace.log_task(
                LogLevel::Error,
                "executor",
                &format!("Task '{}': failed to spawn process: {}", task_name, e),
            );
            // Write the error to output.log so it's visible in the UI
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(output_log_path) {
                let _ = writeln!(f, "[stderr] Failed to spawn '{}': {}", spec.program, e);
            }
            return CommandResult {
                status: RunStatus::Failed,
                exit_code: None,
            };
        }
    };

    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': process spawned (PID {})", task_name, child.id()),
    );

    // Open output log writer (shared between stdout/stderr reader threads)
    let log_writer = open_live_log(output_log_path);

    // Take stdout/stderr handles and spawn streaming reader threads (disk-only, no RAM buffer)
    let stdout_handle = child.stdout.take().map(|s| {
        spawn_stream_reader(s, "stdout", log_writer.clone())
    });
    let stderr_handle = child.stderr.take().map(|s| {
        spawn_stream_reader(s, "stderr", log_writer)
    });

    // Wait for the process to exit, checking both timeout and cancellation
    let pid = child.id();
    let exit_result = wait_with_cancel(&mut child, timeout, cancel, pid);

    // Join the reader threads to ensure all output is flushed to disk
    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    match exit_result {
        Ok(status) => {
            let exit_code = status.code();
            let run_status = if status.success() {
                RunStatus::Passed
            } else {
                RunStatus::Failed
            };
            CommandResult {
                status: run_status,
                exit_code,
            }
        }
        Err(WaitError::Cancelled) => {
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!("Task '{}': stopped by user", task_name),
            );
            CommandResult {
                status: RunStatus::Stopped,
                exit_code: None,
            }
        }
        Err(WaitError::Timeout) => {
            workspace.log_task(
                LogLevel::Warn,
                "executor",
                &format!("Task '{}': process timed out after {:?}", task_name, timeout.unwrap_or_default()),
            );
            CommandResult {
                status: RunStatus::Timeout,
                exit_code: None,
            }
        }
        Err(WaitError::Other) => {
            CommandResult {
                status: RunStatus::Failed,
                exit_code: None,
            }
        }
    }
}

fn wait_with_cancel(
    child: &mut std::process::Child,
    timeout: Option<Duration>,
    cancel: &CancelToken,
    pid: u32,
) -> Result<std::process::ExitStatus, WaitError> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) {
                    kill_process_tree(pid);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(WaitError::Cancelled);
                }
                if let Some(t) = timeout {
                    if start.elapsed() >= t {
                        kill_process_tree(pid);
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(WaitError::Timeout);
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Err(WaitError::Other),
        }
    }
}

/// Kill a process and all its descendants.
#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();
}

#[cfg(not(windows))]
fn kill_process_tree(_pid: u32) {
    // Non-Windows: child.kill() in the caller handles the direct process
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}
