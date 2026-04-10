use serde::{Deserialize, Serialize};
use std::path::Path;

/// Shell used to execute task commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    /// `cmd /C` (Windows default)
    Cmd,
    /// `powershell.exe -NoProfile -NonInteractive -Command`
    #[serde(alias = "powershell")]
    PowerShell,
    /// `pwsh.exe -NoProfile -NonInteractive -Command` (PowerShell 6+/Core)
    Pwsh,
    /// `sh -c` (Unix default)
    Sh,
    /// `bash -c`
    Bash,
}

impl Shell {
    /// Platform default shell.
    pub fn platform_default() -> Self {
        if cfg!(target_os = "windows") {
            Shell::PowerShell
        } else {
            Shell::Sh
        }
    }
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shell::Cmd => write!(f, "cmd"),
            Shell::PowerShell => write!(f, "powershell"),
            Shell::Pwsh => write!(f, "pwsh"),
            Shell::Sh => write!(f, "sh"),
            Shell::Bash => write!(f, "bash"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub updates: UpdateConfig,
    #[serde(default, rename = "task")]
    pub tasks: Vec<TaskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_max_retention")]
    pub max_log_retention_days: u32,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub task_sources: Vec<String>,
    #[serde(default)]
    pub task_configs: Vec<String>,
    #[serde(default)]
    pub default_shell: Option<Shell>,
}

fn default_log_level() -> String {
    "info".to_string()
}
fn default_max_retention() -> u32 {
    30
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            max_log_retention_days: default_max_retention(),
            start_with_windows: false,
            task_sources: Vec::new(),
            task_configs: Vec::new(),
            default_shell: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub on_failure: bool,
    #[serde(default = "default_true")]
    pub on_recovery: bool,
    #[serde(default)]
    pub sound: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            on_failure: true,
            on_recovery: true,
            sound: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_true")]
    pub auto_check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { auto_check: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub name: String,
    pub command: String,
    pub cron: String,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default = "default_true")]
    pub notify_on_failure: bool,
    #[serde(default)]
    pub retries: Option<u32>,
    #[serde(default = "default_true")]
    pub run_missed: bool,
    #[serde(default)]
    pub shell: Option<Shell>,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
    }

    pub fn default_config() -> Self {
        Self {
            general: GeneralConfig::default(),
            notifications: NotificationConfig::default(),
            updates: UpdateConfig::default(),
            tasks: vec![
                TaskConfig {
                    name: "example-hello".to_string(),
                    command: "Write-Output 'Hello from TaskPilot!'".to_string(),
                    cron: "*/5 * * * *".to_string(),
                    timeout: Some("30s".to_string()),
                    working_dir: None,
                    notify_on_failure: true,
                    retries: None,
                    run_missed: true,
                    shell: None,
                },
                TaskConfig {
                    name: "example-date".to_string(),
                    command: "Get-Date -Format 'yyyy-MM-dd'".to_string(),
                    cron: "*/2 * * * *".to_string(),
                    timeout: Some("10s".to_string()),
                    working_dir: None,
                    notify_on_failure: true,
                    retries: None,
                    run_missed: true,
                    shell: None,
                },
            ],
        }
    }

    pub fn save_default(path: &Path) -> Result<(), String> {
        let config = Self::default_config();
        let content = toml::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        std::fs::write(path, content).map_err(|e| format!("Failed to write config: {}", e))
    }
}
