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
    pub stdout: String,
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

    pub fn live_log_path(&self, task_name: &str) -> PathBuf {
        self.task_runs_dir(task_name).join("live.log")
    }

    pub fn read_live_log(&self, task_name: &str) -> String {
        let path = self.live_log_path(task_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    pub fn remove_live_log(&self, task_name: &str) {
        let path = self.live_log_path(task_name);
        let _ = std::fs::remove_file(&path);
    }

    pub fn save_run(&self, run: &TaskRun) -> Result<PathBuf, String> {
        let dir = self.task_runs_dir(&run.task_name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create task runs dir: {}", e))?;

        let filename = run.started_at.format("%Y-%m-%dT%H%M%S.json").to_string();
        let path = dir.join(&filename);

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

        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map_or(false, |ext| ext == "json"))
                    .collect()
            })
            .unwrap_or_default();

        files.sort_by(|a, b| b.cmp(a)); // newest first
        files.truncate(limit);

        files
            .iter()
            .filter_map(|path| {
                let content = std::fs::read_to_string(path).ok()?;
                serde_json::from_str(&content).ok()
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
