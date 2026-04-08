use crate::config::TaskConfig;
use crate::logging::LogLevel;
use crate::workspace::{TaskRun, RunStatus, Workspace};
use chrono::Utc;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

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

    workspace.log_task(
        LogLevel::Info,
        "executor",
        &format!("Starting task '{}': command=\"{}\"", task.name, task.command),
    );
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!(
            "Task '{}' config: cron=\"{}\", timeout={}, working_dir={}, retries={}, notify_on_failure={}",
            task.name,
            task.cron,
            task.timeout.as_deref().unwrap_or("none"),
            task.working_dir.as_deref().unwrap_or("(default)"),
            retries,
            task.notify_on_failure,
        ),
    );
    if let Some(ref dur) = timeout {
        workspace.log_task(
            LogLevel::Debug,
            "executor",
            &format!("Task '{}': parsed timeout = {:?}", task.name, dur),
        );
    }

    // Ensure the task runs directory exists for the live log
    let _ = std::fs::create_dir_all(workspace.task_runs_dir(&task.name));
    let live_log_path = workspace.live_log_path(&task.name);
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': live log path = \"{}\"", task.name, live_log_path.display()),
    );

    let mut last_run = None;

    for attempt in 0..=retries {
        if attempt > 0 {
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!("Task '{}': retry attempt {}/{}", task.name, attempt, retries),
            );
        }

        // Clear live log at start of each attempt
        let _ = std::fs::write(&live_log_path, b"");

        let result = run_command(
            &task.command,
            task.working_dir.as_deref(),
            timeout,
            Some(&live_log_path),
            workspace,
            &task.name,
        );
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
            workspace.log_task(
                LogLevel::Info,
                "executor",
                &format!(
                    "Task '{}': completed status={} exit_code={} duration={}ms",
                    task.name,
                    match run.status {
                        RunStatus::Passed => "passed",
                        RunStatus::Failed => "failed",
                        RunStatus::Timeout => "timeout",
                        RunStatus::Running => "running",
                    },
                    run.exit_code.map_or("none".to_string(), |c| c.to_string()),
                    elapsed.as_millis(),
                ),
            );
            let _ = workspace.save_run(&run);
            workspace.remove_live_log(&task.name);
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

/// Shared writer for the live log file, used by stdout/stderr reader threads.
type LiveLogWriter = Arc<Mutex<std::io::BufWriter<std::fs::File>>>;

/// Create a live log writer for the given path. Returns None if the file can't be opened.
fn open_live_log(path: &Path) -> Option<LiveLogWriter> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .ok()
        .map(|f| Arc::new(Mutex::new(std::io::BufWriter::new(f))))
}

/// Spawn a thread that reads lines from `reader` and appends them to both an in-memory
/// buffer and the live log file (if provided). Returns a join handle; the buffer is
/// collected via the Arc once the thread finishes.
fn spawn_stream_reader<R: Read + Send + 'static>(
    reader: R,
    prefix: &str,
    log_writer: Option<LiveLogWriter>,
) -> (std::thread::JoinHandle<()>, Arc<Mutex<Vec<u8>>>) {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let buf_clone = Arc::clone(&buf);
    let prefix = prefix.to_string();

    let handle = std::thread::spawn(move || {
        let mut br = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match br.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    // Append to in-memory buffer
                    if let Ok(mut b) = buf_clone.lock() {
                        b.extend_from_slice(line.as_bytes());
                    }
                    // Append to live log file
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
    });

    (handle, buf)
}

fn run_command(
    command: &str,
    working_dir: Option<&str>,
    timeout: Option<Duration>,
    live_log_path: Option<&Path>,
    workspace: &Workspace,
    task_name: &str,
) -> CommandResult {
    let (shell, shell_flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': shell command = {} {} \"{}\"", task_name, shell, shell_flag, command),
    );

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        // Use raw_arg to avoid Rust escaping double quotes in the command string.
        // cmd.exe doesn't understand \" — it uses its own quote handling rules.
        #[cfg(windows)]
        {
            c.raw_arg("/C");
            c.raw_arg(command);
            c.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        #[cfg(not(windows))]
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };

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
            return CommandResult {
                status: RunStatus::Failed,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to spawn process: {}", e),
            };
        }
    };

    workspace.log_task(
        LogLevel::Debug,
        "executor",
        &format!("Task '{}': process spawned (PID {})", task_name, child.id()),
    );

    // Open live log writer (shared between stdout/stderr reader threads)
    let log_writer = live_log_path.and_then(open_live_log);

    // Take stdout/stderr handles and spawn streaming reader threads
    let stdout_handle = child.stdout.take().map(|s| {
        spawn_stream_reader(s, "stdout", log_writer.clone())
    });
    let stderr_handle = child.stderr.take().map(|s| {
        spawn_stream_reader(s, "stderr", log_writer)
    });

    // Wait for the process to exit (with or without timeout)
    let exit_result = if let Some(timeout_dur) = timeout {
        wait_with_timeout(&mut child, timeout_dur)
    } else {
        child.wait().map_err(|_| ())
    };

    // Join the reader threads to collect output
    let stdout_str = stdout_handle
        .and_then(|(h, buf)| {
            let _ = h.join();
            buf.lock().ok().map(|b| String::from_utf8_lossy(&b).to_string())
        })
        .unwrap_or_default();

    let stderr_str = stderr_handle
        .and_then(|(h, buf)| {
            let _ = h.join();
            buf.lock().ok().map(|b| String::from_utf8_lossy(&b).to_string())
        })
        .unwrap_or_default();

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
                stdout: stdout_str,
                stderr: stderr_str,
            }
        }
        Err(_) => {
            workspace.log_task(
                LogLevel::Warn,
                "executor",
                &format!("Task '{}': process timed out after {:?}", task_name, timeout.unwrap_or_default()),
            );
            CommandResult {
                status: RunStatus::Timeout,
                exit_code: None,
                stdout: stdout_str,
                stderr: if stderr_str.is_empty() {
                    format!("Process timed out after {:?}", timeout.unwrap_or_default())
                } else {
                    format!("{}\nProcess timed out after {:?}", stderr_str, timeout.unwrap_or_default())
                },
            }
        }
    }
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, ()> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(status);
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
