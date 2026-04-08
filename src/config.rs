use serde::{Deserialize, Serialize};
use std::path::Path;

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
                    command: "echo Hello from TaskPilot!".to_string(),
                    cron: "*/5 * * * *".to_string(),
                    timeout: Some("30s".to_string()),
                    working_dir: None,
                    notify_on_failure: true,
                    retries: None,
                },
                TaskConfig {
                    name: "example-date".to_string(),
                    command: "date /t".to_string(),
                    cron: "*/2 * * * *".to_string(),
                    timeout: Some("10s".to_string()),
                    working_dir: None,
                    notify_on_failure: true,
                    retries: None,
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
