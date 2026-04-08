use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl LogLevel {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => LogLevel::Error,
            1 => LogLevel::Warn,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            _ => LogLevel::Info,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

pub fn parse_log_level(s: &str) -> LogLevel {
    match s.trim().to_lowercase().as_str() {
        "error" => LogLevel::Error,
        "warn" | "warning" => LogLevel::Warn,
        "info" => LogLevel::Info,
        "debug" => LogLevel::Debug,
        _ => LogLevel::Info,
    }
}

/// Thread-safe atomic log level for use in `Workspace`.
pub struct AtomicLogLevel(AtomicU8);

impl AtomicLogLevel {
    pub fn new(level: LogLevel) -> Self {
        Self(AtomicU8::new(level.as_u8()))
    }

    pub fn get(&self) -> LogLevel {
        LogLevel::from_u8(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, level: LogLevel) {
        self.0.store(level.as_u8(), Ordering::Relaxed);
    }
}

impl Default for AtomicLogLevel {
    fn default() -> Self {
        Self::new(LogLevel::Info)
    }
}
