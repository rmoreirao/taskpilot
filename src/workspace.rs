use crate::logging::{AtomicLogLevel, LogLevel};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Passed,
    Failed,
    Running,
    Timeout,
    Stopped,
}

/// Snapshot of the task configuration at execution time, stored alongside each run
/// for post-mortem debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunConfig {
    pub command: String,
    pub cron: String,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub notify_on_failure: bool,
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub run_missed: bool,
    pub shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub task_name: String,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    /// Legacy fields kept for backward compatibility with old JSON run files.
    /// New runs do NOT populate these — output lives in output.log alongside run.json.
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    pub started_at: DateTime<Local>,
    pub finished_at: Option<DateTime<Local>>,
    pub duration_ms: Option<u64>,
    /// Which attempt this run represents (0-based) and total attempts made.
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(default)]
    pub total_attempts: Option<u32>,
    /// Task configuration snapshot captured at execution time.
    #[serde(default)]
    pub config: Option<TaskRunConfig>,
    /// Path to the output.log file for this run (not serialized — set at load time).
    #[serde(skip)]
    pub output_log_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerState {
    pub tasks: std::collections::HashMap<String, TaskScheduleState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskScheduleState {
    pub last_run: Option<DateTime<Local>>,
    pub next_run: Option<DateTime<Local>>,
    pub last_status: Option<RunStatus>,
    #[serde(default)]
    pub cron_expr: Option<String>,
}

pub struct Workspace {
    pub root: PathBuf,
    log_level: AtomicLogLevel,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            log_level: AtomicLogLevel::default(),
        }
    }

    pub fn set_log_level(&self, level: LogLevel) {
        self.log_level.set(level);
    }

    pub fn ensure_dirs(&self) -> Result<(), String> {
        std::fs::create_dir_all(self.runs_dir())
            .map_err(|e| format!("Failed to create runs dir: {}", e))?;
        std::fs::create_dir_all(self.debug_dir())
            .map_err(|e| format!("Failed to create debug dir: {}", e))
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    pub fn debug_dir(&self) -> PathBuf {
        self.root.join("debug")
    }

    pub fn debug_log_path(&self) -> PathBuf {
        self.debug_dir().join("app.log")
    }

    pub fn task_log_path(&self) -> PathBuf {
        self.debug_dir().join("task-runs.log")
    }

    /// Write a level-gated log entry to the task-runs.log file.
    pub fn log_task(&self, level: LogLevel, component: &str, message: &str) {
        if level > self.log_level.get() {
            return;
        }
        let _ = append_leveled_log(&self.task_log_path(), level, component, message);
    }

    pub fn task_runs_dir(&self, task_name: &str) -> PathBuf {
        self.runs_dir().join(sanitize_filename(task_name))
    }

    /// Directory for a specific run, e.g. `.taskpilot/runs/<task>/2026-04-09T193656/`
    pub fn run_dir(&self, task_name: &str, started_at: &DateTime<Local>) -> PathBuf {
        let folder = started_at.format("%Y-%m-%dT%H%M%S").to_string();
        self.task_runs_dir(task_name).join(folder)
    }

    /// Path to output.log inside a run directory.
    pub fn output_log_path(&self, task_name: &str, started_at: &DateTime<Local>) -> PathBuf {
        self.run_dir(task_name, started_at).join("output.log")
    }

    /// Read the output log for a running task (for live display).
    pub fn read_output_log(&self, task_name: &str, started_at: &DateTime<Local>) -> String {
        let path = self.output_log_path(task_name, started_at);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    /// Read the tail of the output log (last `max_bytes` bytes), useful for notifications.
    pub fn read_output_log_tail(&self, task_name: &str, started_at: &DateTime<Local>, max_bytes: usize) -> String {
        let path = self.output_log_path(task_name, started_at);
        read_file_tail(&path, max_bytes)
    }

    /// Read the output log for a run using its stored path (for history display).
    pub fn read_output_log_from_path(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub fn save_run(&self, run: &TaskRun) -> Result<PathBuf, String> {
        let dir = self.run_dir(&run.task_name, &run.started_at);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create run dir: {}", e))?;

        let path = dir.join("run.json");

        let json = serde_json::to_string_pretty(run)
            .map_err(|e| format!("Failed to serialize run: {}", e))?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to write run file: {}", e))?;

        Ok(path)
    }

    pub fn load_runs(&self, task_name: &str, limit: usize) -> Vec<TaskRun> {
        let dir = self.task_runs_dir(task_name);
        if !dir.exists() {
            return Vec::new();
        }

        let entries: Vec<_> = std::fs::read_dir(&dir)
            .ok()
            .map(|entries| entries.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();

        // Collect (sort_key, json_path, output_log_path) from both old and new formats
        let mut run_entries: Vec<(String, PathBuf, Option<PathBuf>)> = Vec::new();

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                // New format: subfolder with run.json + output.log
                let json_path = path.join("run.json");
                if json_path.exists() {
                    let sort_key = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let output_path = path.join("output.log");
                    let output = if output_path.exists() { Some(output_path) } else { None };
                    run_entries.push((sort_key, json_path, output));
                }
            } else if path.extension().map_or(false, |ext| ext == "json") {
                // Old format: flat .json file with embedded stdout/stderr
                let sort_key = path.file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                run_entries.push((sort_key, path, None));
            }
        }

        run_entries.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
        run_entries.truncate(limit);

        run_entries
            .into_iter()
            .filter_map(|(_, json_path, output_path)| {
                let content = std::fs::read_to_string(&json_path).ok()?;
                let mut run: TaskRun = serde_json::from_str(&content).ok()?;
                run.output_log_path = output_path;
                Some(run)
            })
            .collect()
    }

    pub fn get_latest_run(&self, task_name: &str) -> Option<TaskRun> {
        self.load_runs(task_name, 1).into_iter().next()
    }

    pub fn load_state(&self) -> SchedulerState {
        let path = self.state_path();
        if !path.exists() {
            return SchedulerState::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_state(&self, state: &SchedulerState) -> Result<(), String> {
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;
        std::fs::write(self.state_path(), json)
            .map_err(|e| format!("Failed to write state: {}", e))
    }

    pub fn config_content(&self) -> String {
        std::fs::read_to_string(self.config_path()).unwrap_or_default()
    }

    pub fn append_debug_log(&self, component: &str, message: &str) -> Result<(), String> {
        append_debug_log(&self.debug_log_path(), component, message)
    }
}

pub fn append_debug_log(path: &Path, component: &str, message: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create debug log directory: {}", e))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open debug log: {}", e))?;

    let line = format!(
        "[{}] [{}] {}\n",
        Local::now().to_rfc3339(),
        component,
        message
    );

    file.write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write debug log: {}", e))
}

pub fn append_leveled_log(
    path: &Path,
    level: LogLevel,
    component: &str,
    message: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open log: {}", e))?;

    let line = format!(
        "[{}] [{}] [{}] {}\n",
        Local::now().to_rfc3339(),
        level.label(),
        component,
        message
    );

    file.write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write log: {}", e))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Read the last `max_bytes` bytes from a file as a String.
fn read_file_tail(path: &Path, max_bytes: usize) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0) as usize;
    if len > max_bytes {
        let _ = file.seek(SeekFrom::End(-(max_bytes as i64)));
    }
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf);
    buf
}
