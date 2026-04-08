use crate::config::TaskConfig;
use crate::logging::LogLevel;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum TaskOrigin {
    Local,
    External { dir: PathBuf },
}

#[derive(Debug, Clone)]
pub struct TaskSourceInfo {
    pub origin: TaskOrigin,
    pub file_path: PathBuf,
}

impl TaskSourceInfo {
    pub fn is_external(&self) -> bool {
        matches!(self.origin, TaskOrigin::External { .. })
    }

    pub fn source_label(&self) -> String {
        match &self.origin {
            TaskOrigin::Local => "local".to_string(),
            TaskOrigin::External { dir } => dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.to_string_lossy().to_string()),
        }
    }
}

/// Intermediate struct for parsing TOML files that contain a `[[task]]` array.
#[derive(serde::Deserialize)]
struct MultiTaskFile {
    #[serde(default, rename = "task")]
    tasks: Vec<TaskConfig>,
}

/// Load all `.toml` files from a directory, returning tasks with their source file.
pub fn load_dir(dir: &Path, workspace: Option<&Workspace>) -> Result<Vec<(TaskConfig, PathBuf)>, String> {
    if !dir.exists() {
        return Err(format!("Task source directory does not exist: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("Task source path is not a directory: {}", dir.display()));
    }

    if let Some(ws) = workspace {
        ws.log_task(LogLevel::Info, "sources", &format!("Scanning external task source: {}", dir.display()));
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    let mut tasks = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().map_or(true, |ext| ext != "toml") {
            continue;
        }

        if let Some(ws) = workspace {
            ws.log_task(LogLevel::Debug, "sources", &format!("Found TOML file: {}", path.display()));
        }

        match load_toml_file(&path) {
            Ok(file_tasks) => {
                if let Some(ws) = workspace {
                    let format = if file_tasks.len() > 1 { "multi-task" } else { "single-task" };
                    ws.log_task(
                        LogLevel::Debug,
                        "sources",
                        &format!("Parsed {} format from {}: {} task(s)", format, path.display(), file_tasks.len()),
                    );
                }
                for task in file_tasks {
                    tasks.push((task, path.clone()));
                }
            }
            Err(e) => {
                if let Some(ws) = workspace {
                    ws.log_task(LogLevel::Warn, "sources", &format!("Skipping {}: {}", path.display(), e));
                }
                eprintln!("Warning: skipping {}: {}", path.display(), e);
            }
        }
    }

    if let Some(ws) = workspace {
        ws.log_task(
            LogLevel::Info,
            "sources",
            &format!("Loaded {} tasks from external source {}", tasks.len(), dir.display()),
        );
    }

    Ok(tasks)
}

/// Parse a single `.toml` file, trying multi-task format first, then single-task.
fn load_toml_file(path: &Path) -> Result<Vec<TaskConfig>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    // Try multi-task format: file has a [[task]] array
    if let Ok(multi) = toml::from_str::<MultiTaskFile>(&content) {
        if !multi.tasks.is_empty() {
            return Ok(multi.tasks);
        }
    }

    // Try single-task format: flat name/command/cron fields
    if let Ok(single) = toml::from_str::<TaskConfig>(&content) {
        return Ok(vec![single]);
    }

    Err(format!(
        "Could not parse {} as either multi-task or single-task TOML",
        path.display()
    ))
}

/// Merge local tasks with tasks from external source directories.
/// Returns the merged task list and a map of task name → source info.
/// Errors if any task names collide across sources.
pub fn load_all(
    local_tasks: &[TaskConfig],
    config_path: &Path,
    source_dirs: &[PathBuf],
    workspace: Option<&Workspace>,
) -> Result<(Vec<TaskConfig>, HashMap<String, TaskSourceInfo>), String> {
    let mut all_tasks: Vec<TaskConfig> = Vec::new();
    let mut source_map: HashMap<String, TaskSourceInfo> = HashMap::new();

    // Register local tasks
    for task in local_tasks {
        if source_map.contains_key(&task.name) {
            let msg = format!("Duplicate task name '{}' in local config", task.name);
            if let Some(ws) = workspace {
                ws.log_task(LogLevel::Error, "sources", &msg);
            }
            return Err(msg);
        }
        source_map.insert(
            task.name.clone(),
            TaskSourceInfo {
                origin: TaskOrigin::Local,
                file_path: config_path.to_path_buf(),
            },
        );
        all_tasks.push(task.clone());
    }

    // Load and register external tasks
    let mut external_count = 0usize;
    for dir in source_dirs {
        let dir_path = PathBuf::from(expand_home(&dir.to_string_lossy()));
        let external_tasks = load_dir(&dir_path, workspace)?;

        for (task, file_path) in external_tasks {
            if let Some(existing) = source_map.get(&task.name) {
                let existing_source = match &existing.origin {
                    TaskOrigin::Local => "local config".to_string(),
                    TaskOrigin::External { dir } => format!("{}", dir.display()),
                };
                let msg = format!(
                    "Duplicate task name '{}': defined in {} and {}",
                    task.name,
                    existing_source,
                    file_path.display(),
                );
                if let Some(ws) = workspace {
                    ws.log_task(LogLevel::Error, "sources", &msg);
                }
                return Err(msg);
            }
            source_map.insert(
                task.name.clone(),
                TaskSourceInfo {
                    origin: TaskOrigin::External {
                        dir: dir_path.clone(),
                    },
                    file_path,
                },
            );
            all_tasks.push(task);
            external_count += 1;
        }
    }

    if let Some(ws) = workspace {
        ws.log_task(
            LogLevel::Info,
            "sources",
            &format!(
                "Task merge complete: {} local + {} external = {} tasks",
                local_tasks.len(),
                external_count,
                all_tasks.len(),
            ),
        );
    }

    Ok((all_tasks, source_map))
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}
