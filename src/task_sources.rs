use crate::config::TaskConfig;
use crate::config_diagnostics::{ConfigIssue, ConfigIssueSeverity};
use crate::timezone;
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

#[derive(Debug, Clone)]
pub struct TaskSourceLoadOutcome {
    pub tasks: Vec<TaskConfig>,
    pub source_metadata: HashMap<String, TaskSourceInfo>,
    pub diagnostics: Vec<ConfigIssue>,
}

#[derive(Debug, Clone)]
pub struct TaskSourceLoadError {
    pub diagnostics: Vec<ConfigIssue>,
}

impl TaskSourceLoadError {
    pub fn primary_message(&self) -> String {
        self.diagnostics
            .iter()
            .find(|issue| issue.severity == ConfigIssueSeverity::Error)
            .or_else(|| self.diagnostics.first())
            .map(|issue| issue.line())
            .unwrap_or_else(|| "Task source loading failed".to_string())
    }
}

struct DirLoadOutcome {
    tasks: Vec<(TaskConfig, PathBuf)>,
    diagnostics: Vec<ConfigIssue>,
}

/// Load all `.toml` files from a directory, returning tasks with their source file.
pub fn load_dir(
    dir: &Path,
    workspace: Option<&Workspace>,
) -> Result<Vec<(TaskConfig, PathBuf)>, String> {
    match load_dir_detailed(dir) {
        Ok(outcome) => {
            emit_diagnostics(workspace, &outcome.diagnostics);
            emit_warning_stderr(&outcome.diagnostics);
            Ok(outcome.tasks)
        }
        Err(err) => {
            emit_diagnostics(workspace, &err.diagnostics);
            emit_warning_stderr(&err.diagnostics);
            Err(err.primary_message())
        }
    }
}

fn load_dir_detailed(dir: &Path) -> Result<DirLoadOutcome, TaskSourceLoadError> {
    if !dir.exists() {
        return Err(TaskSourceLoadError {
            diagnostics: vec![ConfigIssue::error(
                source_dir_label(dir),
                "Directory does not exist".to_string(),
            )],
        });
    }
    if !dir.is_dir() {
        return Err(TaskSourceLoadError {
            diagnostics: vec![ConfigIssue::error(
                source_dir_label(dir),
                "Path is not a directory".to_string(),
            )],
        });
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| TaskSourceLoadError {
            diagnostics: vec![ConfigIssue::error(
                source_dir_label(dir),
                format!("Failed to read directory: {}", e),
            )],
        })?;

    let mut tasks = Vec::new();
    let mut diagnostics = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                diagnostics.push(ConfigIssue::warning(
                    source_dir_label(dir),
                    format!("Failed to read a directory entry: {}", e),
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().map_or(true, |ext| ext != "toml") {
            continue;
        }

        match load_file(&path) {
            Ok(file_tasks) => {
                for task in file_tasks {
                    tasks.push((task, path.clone()));
                }
            }
            Err(e) => {
                diagnostics.push(ConfigIssue::warning(source_file_label(&path), e));
            }
        }
    }

    Ok(DirLoadOutcome { tasks, diagnostics })
}

/// Parse a single `.toml` file, trying multi-task format first, then single-task.
pub fn load_file(path: &Path) -> Result<Vec<TaskConfig>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Try multi-task format: file has a [[task]] array
    let multi_result = toml::from_str::<MultiTaskFile>(&content);
    let multi_detail = match multi_result {
        Ok(multi) if !multi.tasks.is_empty() => return Ok(multi.tasks),
        Ok(_) => "multi-task format contained no [[task]] entries".to_string(),
        Err(err) => format!("multi-task parse error: {}", err),
    };

    // Try single-task format: flat name/command/cron fields
    match toml::from_str::<TaskConfig>(&content) {
        Ok(single) => Ok(vec![single]),
        Err(single_err) => Err(format!(
            "Could not parse as either multi-task or single-task TOML; {}; single-task parse error: {}",
            multi_detail, single_err
        )),
    }
}

/// Merge local tasks with tasks from external source directories.
/// Returns the merged task list and a map of task name → source info.
/// Errors if any task names collide across sources.
pub fn load_all(
    local_tasks: &[TaskConfig],
    config_path: &Path,
    source_dirs: &[PathBuf],
    source_files: &[PathBuf],
    default_timezone: Option<&str>,
    workspace: Option<&Workspace>,
) -> Result<(Vec<TaskConfig>, HashMap<String, TaskSourceInfo>), String> {
    match load_all_detailed(
        local_tasks,
        config_path,
        source_dirs,
        source_files,
        default_timezone,
    ) {
        Ok(outcome) => {
            emit_diagnostics(workspace, &outcome.diagnostics);
            emit_warning_stderr(&outcome.diagnostics);
            Ok((outcome.tasks, outcome.source_metadata))
        }
        Err(err) => {
            emit_diagnostics(workspace, &err.diagnostics);
            emit_warning_stderr(&err.diagnostics);
            Err(err.primary_message())
        }
    }
}

pub fn load_all_detailed(
    local_tasks: &[TaskConfig],
    config_path: &Path,
    source_dirs: &[PathBuf],
    source_files: &[PathBuf],
    default_timezone: Option<&str>,
) -> Result<TaskSourceLoadOutcome, TaskSourceLoadError> {
    let mut all_tasks: Vec<TaskConfig> = Vec::new();
    let mut source_map: HashMap<String, TaskSourceInfo> = HashMap::new();
    let mut diagnostics = Vec::new();

    // Register local tasks
    for task in local_tasks {
        if source_map.contains_key(&task.name) {
            diagnostics.push(ConfigIssue::error(
                main_config_label(config_path),
                format!("Duplicate task name '{}' in the main config", task.name),
            ));
            return Err(TaskSourceLoadError { diagnostics });
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
    for dir in source_dirs {
        let dir_path = PathBuf::from(expand_home(&dir.to_string_lossy()));
        let dir_outcome = match load_dir_detailed(&dir_path) {
            Ok(outcome) => outcome,
            Err(mut err) => {
                diagnostics.append(&mut err.diagnostics);
                return Err(TaskSourceLoadError { diagnostics });
            }
        };
        diagnostics.extend(dir_outcome.diagnostics);

        for (task, file_path) in dir_outcome.tasks {
            if let Some(existing) = source_map.get(&task.name) {
                diagnostics.push(ConfigIssue::error(
                    source_file_label(&file_path),
                    format!(
                        "Duplicate task name '{}' is already defined in {}",
                        task.name,
                        existing.file_path.display()
                    ),
                ));
                return Err(TaskSourceLoadError { diagnostics });
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
        }
    }

    // Load and register tasks from individual source files
    for file in source_files {
        let file_path = PathBuf::from(expand_home(&file.to_string_lossy()));

        if !file_path.exists() {
            diagnostics.push(ConfigIssue::warning(
                source_file_label(&file_path),
                "File does not exist".to_string(),
            ));
            continue;
        }

        match load_file(&file_path) {
            Ok(file_tasks) => {
                for task in file_tasks {
                    if let Some(existing) = source_map.get(&task.name) {
                        diagnostics.push(ConfigIssue::error(
                            source_file_label(&file_path),
                            format!(
                                "Duplicate task name '{}' is already defined in {}",
                                task.name,
                                existing.file_path.display()
                            ),
                        ));
                        return Err(TaskSourceLoadError { diagnostics });
                    }
                    let parent_dir = file_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| file_path.clone());
                    source_map.insert(
                        task.name.clone(),
                        TaskSourceInfo {
                            origin: TaskOrigin::External {
                                dir: parent_dir,
                            },
                            file_path: file_path.clone(),
                        },
                    );
                    all_tasks.push(task);
                }
            }
            Err(e) => {
                diagnostics.push(ConfigIssue::warning(source_file_label(&file_path), e));
            }
        }
    }

    // Validate triggers: targets must exist, no self-triggers, no cycles
    if let Err(msg) = validate_triggers(&all_tasks) {
        diagnostics.push(ConfigIssue::error(validation_label(), msg));
        return Err(TaskSourceLoadError { diagnostics });
    }
    if let Err(msg) = timezone::validate_tasks_timezones(&all_tasks, default_timezone) {
        diagnostics.push(ConfigIssue::error(validation_label(), msg));
        return Err(TaskSourceLoadError { diagnostics });
    }

    Ok(TaskSourceLoadOutcome {
        tasks: all_tasks,
        source_metadata: source_map,
        diagnostics,
    })
}

/// Validate that all trigger targets exist and the trigger graph is acyclic.
fn validate_triggers(tasks: &[TaskConfig]) -> Result<(), String> {
    let task_names: std::collections::HashSet<&str> =
        tasks.iter().map(|t| t.name.as_str()).collect();

    // Check each trigger target exists and no self-triggers
    for task in tasks {
        for trigger in &task.triggers {
            if trigger.task == task.name {
                return Err(format!("Task '{}' triggers itself", task.name));
            }
            if !task_names.contains(trigger.task.as_str()) {
                return Err(format!(
                    "Task '{}' has trigger for unknown task '{}'",
                    task.name, trigger.task
                ));
            }
        }
    }

    // Build adjacency list and detect cycles via DFS
    let adj: HashMap<&str, Vec<&str>> = tasks
        .iter()
        .map(|t| {
            let targets: Vec<&str> = t.triggers.iter().map(|tr| tr.task.as_str()).collect();
            (t.name.as_str(), targets)
        })
        .collect();

    // DFS cycle detection: 0=unvisited, 1=in-stack, 2=done
    let mut state: HashMap<&str, u8> = task_names.iter().map(|&n| (n, 0u8)).collect();
    let mut path: Vec<&str> = Vec::new();

    for &name in &task_names {
        if state[name] == 0 {
            if let Some(cycle) = dfs_find_cycle(name, &adj, &mut state, &mut path) {
                return Err(format!("Trigger cycle detected: {}", cycle.join(" → ")));
            }
        }
    }

    Ok(())
}

fn dfs_find_cycle<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    state: &mut HashMap<&'a str, u8>,
    path: &mut Vec<&'a str>,
) -> Option<Vec<String>> {
    state.insert(node, 1); // in-stack
    path.push(node);

    if let Some(neighbors) = adj.get(node) {
        for &next in neighbors {
            match state.get(next).copied().unwrap_or(0) {
                1 => {
                    // Found cycle — extract the cycle portion of the path
                    let cycle_start = path.iter().position(|&n| n == next).unwrap();
                    let mut cycle: Vec<String> = path[cycle_start..].iter().map(|s| s.to_string()).collect();
                    cycle.push(next.to_string());
                    return Some(cycle);
                }
                0 => {
                    if let Some(cycle) = dfs_find_cycle(next, adj, state, path) {
                        return Some(cycle);
                    }
                }
                _ => {} // already done
            }
        }
    }

    path.pop();
    state.insert(node, 2); // done
    None
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

fn emit_diagnostics(workspace: Option<&Workspace>, diagnostics: &[ConfigIssue]) {
    if let Some(ws) = workspace {
        for issue in diagnostics {
            issue.log(ws);
        }
    }
}

fn emit_warning_stderr(diagnostics: &[ConfigIssue]) {
    for issue in diagnostics {
        if issue.severity == ConfigIssueSeverity::Warning {
            eprintln!("Warning: {}", issue.line());
        }
    }
}

fn main_config_label(path: &Path) -> String {
    format!("Main config ({})", path.display())
}

fn source_dir_label(path: &Path) -> String {
    format!("Task source directory ({})", path.display())
}

fn source_file_label(path: &Path) -> String {
    format!("Task config file ({})", path.display())
}

fn validation_label() -> String {
    "Task source validation".to_string()
}
