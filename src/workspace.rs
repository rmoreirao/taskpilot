use chrono::{DateTime, Utc};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub task_name: String,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerState {
    pub tasks: std::collections::HashMap<String, TaskScheduleState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskScheduleState {
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub last_status: Option<RunStatus>,
}

pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
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

    pub fn task_runs_dir(&self, task_name: &str) -> PathBuf {
        self.runs_dir().join(sanitize_filename(task_name))
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
        Utc::now().to_rfc3339(),
        component,
        message
    );

    file.write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write debug log: {}", e))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
