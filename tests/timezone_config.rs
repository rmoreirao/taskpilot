use std::fs;

use taskpilot::config::AppConfig;

#[test]
fn load_rejects_invalid_default_timezone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
default_timezone = "Mars/Olympus_Mons"

[notifications]
enabled = false

[[task]]
name = "hello"
command = "echo hello"
cron = "0 6 * * *"
"#,
    )
    .expect("write config");

    let err = AppConfig::load(&config_path).expect_err("invalid timezone should fail");
    assert!(err.contains("default_timezone"), "unexpected error: {err}");
}

#[test]
fn load_accepts_valid_task_timezone_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
default_timezone = "America/New_York"

[notifications]
enabled = false

[[task]]
name = "hello"
command = "echo hello"
cron = "0 6 * * *"
timezone = "America/Sao_Paulo"
"#,
    )
    .expect("write config");

    let config = AppConfig::load(&config_path).expect("valid timezone config");
    assert_eq!(
        config.general.default_timezone.as_deref(),
        Some("America/New_York")
    );
    assert_eq!(
        config.tasks[0].timezone.as_deref(),
        Some("America/Sao_Paulo")
    );
}
