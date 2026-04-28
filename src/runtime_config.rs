use crate::config::{AppConfig, TaskConfig};
use crate::config_diagnostics::{ConfigAlert, ConfigIssue, ConfigIssueSeverity};
use crate::logging::parse_log_level;
use crate::task_sources::{self, TaskOrigin, TaskSourceInfo};
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimeConfigState {
    pub config: AppConfig,
    pub source_metadata: HashMap<String, TaskSourceInfo>,
    pub source_dirs: Vec<PathBuf>,
    pub source_files: Vec<PathBuf>,
    pub config_alert: Option<ConfigAlert>,
}

#[derive(Debug, Clone)]
pub struct ReloadConfigOutcome {
    pub state: RuntimeConfigState,
    pub applied: bool,
}

pub fn load_startup_state(workspace: &Workspace, cli_task_dirs: &[PathBuf]) -> RuntimeConfigState {
    let config_path = workspace.config_path();
    let mut issues = Vec::new();
    let mut root_config_failed = false;

    let base_config = if config_path.exists() {
        match AppConfig::load(&config_path) {
            Ok(config) => config,
            Err(err) => {
                root_config_failed = true;
                issues.push(ConfigIssue::error(
                    main_config_source(&config_path),
                    format!("Failed to load config: {}", err),
                ));
                AppConfig::default_config()
            }
        }
    } else {
        if let Err(err) = AppConfig::save_default(&config_path) {
            issues.push(ConfigIssue::warning(
                main_config_source(&config_path),
                format!("Failed to write default config: {}", err),
            ));
        }
        AppConfig::default_config()
    };

    let (source_dirs, source_files) = build_source_paths(&base_config, cli_task_dirs);
    let mut effective_config = base_config.clone();
    let mut source_metadata = local_source_metadata(&config_path, &base_config.tasks);
    let mut source_load_failed = false;

    match task_sources::load_all_detailed(
        &base_config.tasks,
        &config_path,
        &source_dirs,
        &source_files,
        base_config.general.default_timezone.as_deref(),
    ) {
        Ok(outcome) => {
            effective_config.tasks = outcome.tasks;
            source_metadata = outcome.source_metadata;
            issues.extend(outcome.diagnostics);
        }
        Err(err) => {
            source_load_failed = true;
            issues.extend(err.diagnostics);
        }
    }

    let alert = if root_config_failed {
        build_alert(
            "Main config failed to load.",
            "TaskPilot is using built-in defaults until the config is fixed.",
            issues,
        )
    } else if source_load_failed {
        build_alert(
            "Referenced task sources failed to load.",
            "TaskPilot is using only tasks from the main config until those sources are fixed.",
            issues,
        )
    } else if !issues.is_empty() {
        build_alert(
            "Config loaded with warnings.",
            "TaskPilot skipped one or more referenced task sources.",
            issues,
        )
    } else {
        None
    };

    let state = RuntimeConfigState {
        config: effective_config,
        source_metadata,
        source_dirs,
        source_files,
        config_alert: alert,
    };

    apply_runtime_logging(workspace, &state);
    state
}

pub fn load_reload_state(
    workspace: &Workspace,
    cli_task_dirs: &[PathBuf],
    current: &RuntimeConfigState,
) -> ReloadConfigOutcome {
    let config_path = workspace.config_path();
    let new_config = match AppConfig::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            let state = state_with_alert(
                current,
                build_alert(
                    "Config reload failed.",
                    "TaskPilot kept the previous config.",
                    vec![ConfigIssue::error(
                        main_config_source(&config_path),
                        format!("Failed to load config: {}", err),
                    )],
                ),
            );
            apply_runtime_logging(workspace, &state);
            return ReloadConfigOutcome {
                state,
                applied: false,
            };
        }
    };

    let (source_dirs, source_files) = build_source_paths(&new_config, cli_task_dirs);
    match task_sources::load_all_detailed(
        &new_config.tasks,
        &config_path,
        &source_dirs,
        &source_files,
        new_config.general.default_timezone.as_deref(),
    ) {
        Ok(outcome) => {
            let mut effective_config = new_config;
            effective_config.tasks = outcome.tasks;
            let alert = if outcome.diagnostics.is_empty() {
                None
            } else {
                build_alert(
                    "Config reloaded with warnings.",
                    "TaskPilot skipped one or more referenced task sources.",
                    outcome.diagnostics,
                )
            };
            let state = RuntimeConfigState {
                config: effective_config,
                source_metadata: outcome.source_metadata,
                source_dirs,
                source_files,
                config_alert: alert,
            };
            apply_runtime_logging(workspace, &state);
            ReloadConfigOutcome {
                state,
                applied: true,
            }
        }
        Err(err) => {
            let state = state_with_alert(
                current,
                build_alert(
                    "Config reload failed.",
                    "TaskPilot kept the previous config.",
                    err.diagnostics,
                ),
            );
            apply_runtime_logging(workspace, &state);
            ReloadConfigOutcome {
                state,
                applied: false,
            }
        }
    }
}

fn apply_runtime_logging(workspace: &Workspace, state: &RuntimeConfigState) {
    workspace.set_log_level(parse_log_level(&state.config.general.log_level));
    if let Some(alert) = &state.config_alert {
        alert.log(workspace);
    }
}

fn build_source_paths(config: &AppConfig, cli_task_dirs: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut source_dirs: Vec<PathBuf> = config
        .general
        .task_sources
        .iter()
        .map(PathBuf::from)
        .collect();
    for dir in cli_task_dirs {
        if !source_dirs.contains(dir) {
            source_dirs.push(dir.clone());
        }
    }

    let source_files = config
        .general
        .task_configs
        .iter()
        .map(PathBuf::from)
        .collect();

    (source_dirs, source_files)
}

fn local_source_metadata(
    config_path: &Path,
    tasks: &[TaskConfig],
) -> HashMap<String, TaskSourceInfo> {
    let mut source_metadata = HashMap::new();
    for task in tasks {
        source_metadata.insert(
            task.name.clone(),
            TaskSourceInfo {
                origin: TaskOrigin::Local,
                file_path: config_path.to_path_buf(),
            },
        );
    }
    source_metadata
}

fn build_alert(
    headline: &str,
    recovery: &str,
    issues: Vec<ConfigIssue>,
) -> Option<ConfigAlert> {
    if issues.is_empty() {
        return None;
    }

    let severity = if issues
        .iter()
        .any(|issue| issue.severity == ConfigIssueSeverity::Error)
    {
        ConfigIssueSeverity::Error
    } else {
        ConfigIssueSeverity::Warning
    };

    Some(ConfigAlert::new(severity, headline, recovery, issues))
}

fn state_with_alert(current: &RuntimeConfigState, alert: Option<ConfigAlert>) -> RuntimeConfigState {
    RuntimeConfigState {
        config: current.config.clone(),
        source_metadata: current.source_metadata.clone(),
        source_dirs: current.source_dirs.clone(),
        source_files: current.source_files.clone(),
        config_alert: alert,
    }
}

fn main_config_source(path: &Path) -> String {
    format!("Main config ({})", path.display())
}
