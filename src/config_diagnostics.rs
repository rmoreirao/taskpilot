use crate::logging::LogLevel;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigIssueSeverity {
    Warning,
    Error,
}

impl ConfigIssueSeverity {
    pub fn label(self) -> &'static str {
        match self {
            ConfigIssueSeverity::Warning => "warning",
            ConfigIssueSeverity::Error => "error",
        }
    }

    pub fn log_level(self) -> LogLevel {
        match self {
            ConfigIssueSeverity::Warning => LogLevel::Warn,
            ConfigIssueSeverity::Error => LogLevel::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    pub severity: ConfigIssueSeverity,
    pub source: String,
    pub message: String,
}

impl ConfigIssue {
    pub fn warning(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ConfigIssueSeverity::Warning,
            source: source.into(),
            message: message.into(),
        }
    }

    pub fn error(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ConfigIssueSeverity::Error,
            source: source.into(),
            message: message.into(),
        }
    }

    pub fn line(&self) -> String {
        format!("{}: {}", self.source, self.message)
    }

    pub fn log(&self, workspace: &Workspace) {
        let line = self.line();
        workspace.log_task(self.severity.log_level(), "config", &line);
        let _ = workspace.append_debug_log(
            "config",
            &format!("{}: {}", self.severity.label().to_uppercase(), line),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAlert {
    pub severity: ConfigIssueSeverity,
    pub headline: String,
    pub recovery: String,
    pub issues: Vec<ConfigIssue>,
}

impl ConfigAlert {
    pub fn new(
        severity: ConfigIssueSeverity,
        headline: impl Into<String>,
        recovery: impl Into<String>,
        issues: Vec<ConfigIssue>,
    ) -> Self {
        Self {
            severity,
            headline: headline.into(),
            recovery: recovery.into(),
            issues,
        }
    }

    pub fn summary_line(&self) -> String {
        if self.recovery.is_empty() {
            self.headline.clone()
        } else {
            format!("{} {}", self.headline, self.recovery)
        }
    }

    pub fn log(&self, workspace: &Workspace) {
        let summary = self.summary_line();
        workspace.log_task(self.severity.log_level(), "config", &summary);
        let _ = workspace.append_debug_log(
            "config",
            &format!("{}: {}", self.severity.label().to_uppercase(), summary),
        );
        for issue in &self.issues {
            issue.log(workspace);
        }
    }
}
