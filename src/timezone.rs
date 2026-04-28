use crate::config::{AppConfig, TaskConfig};
use chrono::{DateTime, Local, Utc};
use chrono_tz::Tz;
use cron::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTimezone {
    Local,
    Named(Tz),
}

impl ResolvedTimezone {
    pub fn label(&self) -> String {
        match self {
            ResolvedTimezone::Local => "system local time".to_string(),
            ResolvedTimezone::Named(tz) => tz.to_string(),
        }
    }

    pub fn key(&self) -> Option<String> {
        match self {
            ResolvedTimezone::Local => None,
            ResolvedTimezone::Named(tz) => Some(tz.to_string()),
        }
    }
}

pub fn validate_timezone_name(name: &str) -> Result<Tz, String> {
    name.parse::<Tz>().map_err(|_| {
        format!(
            "Invalid timezone '{}'. Expected an IANA timezone like 'America/Sao_Paulo'",
            name
        )
    })
}

pub fn validate_app_timezones(config: &AppConfig) -> Result<(), String> {
    validate_tasks_timezones(&config.tasks, config.general.default_timezone.as_deref())
}

pub fn validate_tasks_timezones(
    tasks: &[TaskConfig],
    default_timezone: Option<&str>,
) -> Result<(), String> {
    if let Some(name) = default_timezone {
        validate_timezone_name(name)
            .map_err(|err| format!("Invalid [general].default_timezone: {}", err))?;
    }

    for task in tasks {
        if let Some(name) = &task.timezone {
            validate_timezone_name(name).map_err(|err| {
                format!("Task '{}': invalid timezone '{}': {}", task.name, name, err)
            })?;
        }
    }

    Ok(())
}

pub fn resolve_task_timezone(
    task: &TaskConfig,
    default_timezone: Option<&str>,
) -> Result<ResolvedTimezone, String> {
    match task.timezone.as_deref().or(default_timezone) {
        Some(name) => Ok(ResolvedTimezone::Named(validate_timezone_name(name)?)),
        None => Ok(ResolvedTimezone::Local),
    }
}

pub fn effective_timezone_label(
    task: &TaskConfig,
    default_timezone: Option<&str>,
) -> Result<String, String> {
    Ok(resolve_task_timezone(task, default_timezone)?.label())
}

pub fn next_run_utc(
    schedule: &Schedule,
    timezone: ResolvedTimezone,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match timezone {
        ResolvedTimezone::Local => schedule
            .after(&after.with_timezone(&Local))
            .next()
            .map(|dt| dt.with_timezone(&Utc)),
        ResolvedTimezone::Named(tz) => schedule
            .after(&after.with_timezone(&tz))
            .next()
            .map(|dt| dt.with_timezone(&Utc)),
    }
}

pub fn next_run_local(
    schedule: &Schedule,
    timezone: ResolvedTimezone,
    after: DateTime<Utc>,
) -> Option<DateTime<Local>> {
    next_run_utc(schedule, timezone, after).map(|dt| dt.with_timezone(&Local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TaskConfig;
    use chrono::{TimeZone, Utc};
    use std::str::FromStr;

    fn base_task() -> TaskConfig {
        TaskConfig {
            name: "test".to_string(),
            command: "echo hi".to_string(),
            cron: Some("0 6 * * *".to_string()),
            timeout: None,
            working_dir: None,
            notify_on_failure: true,
            retries: None,
            run_missed: true,
            shell: None,
            timezone: None,
            load_profile: None,
            triggers: Vec::new(),
        }
    }

    #[test]
    fn validates_iana_timezone_name() {
        let tz = validate_timezone_name("America/Sao_Paulo").expect("should parse IANA timezone");
        assert_eq!(tz.to_string(), "America/Sao_Paulo");
    }

    #[test]
    fn task_timezone_overrides_global_default() {
        let mut task = base_task();
        task.timezone = Some("America/Sao_Paulo".to_string());

        let resolved = resolve_task_timezone(&task, Some("America/New_York"))
            .expect("task timezone should resolve");

        assert_eq!(resolved, ResolvedTimezone::Named(Tz::from_str("America/Sao_Paulo").unwrap()));
    }

    #[test]
    fn next_run_uses_winter_offset_for_named_timezone() {
        let schedule = Schedule::from_str("0 0 6 * * *").expect("schedule");
        let after = Utc.with_ymd_and_hms(2026, 1, 10, 8, 30, 0).unwrap();
        let timezone = ResolvedTimezone::Named(Tz::from_str("America/New_York").unwrap());

        let next = next_run_utc(&schedule, timezone, after).expect("next run");

        assert_eq!(next, Utc.with_ymd_and_hms(2026, 1, 10, 11, 0, 0).unwrap());
    }

    #[test]
    fn next_run_uses_summer_offset_for_named_timezone() {
        let schedule = Schedule::from_str("0 0 6 * * *").expect("schedule");
        let after = Utc.with_ymd_and_hms(2026, 7, 10, 8, 30, 0).unwrap();
        let timezone = ResolvedTimezone::Named(Tz::from_str("America/New_York").unwrap());

        let next = next_run_utc(&schedule, timezone, after).expect("next run");

        assert_eq!(next, Utc.with_ymd_and_hms(2026, 7, 10, 10, 0, 0).unwrap());
    }
}
