use std::fs;

use taskpilot::config_diagnostics::ConfigIssueSeverity;
use taskpilot::runtime_config::{load_reload_state, load_startup_state};
use taskpilot::workspace::Workspace;
use tempfile::{tempdir, TempDir};

struct TestWorkspace {
    _tmp: TempDir,
    workspace: Workspace,
}

fn make_workspace() -> TestWorkspace {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join(".taskpilot");
    fs::create_dir_all(&root).expect("workspace dir");
    let workspace = Workspace::new(root);
    workspace.ensure_dirs().expect("ensure dirs");
    TestWorkspace {
        _tmp: dir,
        workspace,
    }
}

#[test]
fn startup_reports_invalid_root_config_and_logs_it() {
    let workspace = make_workspace();
    fs::write(
        workspace.workspace.config_path(),
        "[general]\nlog_level = \"info\"\n[[task]]\nname = \"broken\"\ncommand = ",
    )
    .expect("write config");

    let state = load_startup_state(&workspace.workspace, &[]);
    let alert = state.config_alert.expect("config alert");

    assert_eq!(state.config.tasks[0].name, "example-hello");
    assert_eq!(alert.severity, ConfigIssueSeverity::Error);
    assert!(alert.headline.contains("failed to load"));
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("Failed to load config"))
    );

    let debug_log = fs::read_to_string(workspace.workspace.debug_log_path()).expect("read debug log");
    let task_log = fs::read_to_string(workspace.workspace.task_log_path()).expect("read task log");
    assert!(debug_log.contains("Failed to load config"));
    assert!(task_log.contains("Failed to load config"));
}

#[test]
fn startup_reports_missing_and_broken_referenced_configs() {
    let workspace = make_workspace();
    let missing_path = workspace.workspace.root.join("missing.toml");
    let broken_path = workspace.workspace.root.join("broken.toml");
    let config = format!(
        r#"
[general]
task_configs = [{missing}, {broken}]

[notifications]
enabled = false

[[task]]
name = "local-task"
command = "echo hello"
cron = "*/5 * * * *"
"#,
        missing = toml::Value::String(missing_path.to_string_lossy().to_string()),
        broken = toml::Value::String(broken_path.to_string_lossy().to_string()),
    );
    fs::write(
        workspace.workspace.config_path(),
        config,
    )
    .expect("write config");
    fs::write(&broken_path, "not = [valid").expect("write broken file");

    let state = load_startup_state(&workspace.workspace, &[]);
    let alert = state.config_alert.expect("config alert");

    assert_eq!(state.config.tasks.len(), 1);
    assert_eq!(alert.severity, ConfigIssueSeverity::Warning);
    assert_eq!(alert.issues.len(), 2);
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("does not exist"))
    );
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("multi-task parse error"))
    );
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("single-task parse error"))
    );
}

#[test]
fn startup_falls_back_to_local_tasks_on_duplicate_referenced_task() {
    let workspace = make_workspace();
    let duplicate_path = workspace.workspace.root.join("duplicate.toml");
    let config = format!(
        r#"
[general]
task_configs = [{duplicate}]

[notifications]
enabled = false

[[task]]
name = "local-task"
command = "echo hello"
cron = "*/5 * * * *"
"#,
        duplicate = toml::Value::String(duplicate_path.to_string_lossy().to_string()),
    );
    fs::write(workspace.workspace.config_path(), config).expect("write config");
    fs::write(
        &duplicate_path,
        r#"
name = "local-task"
command = "echo from duplicate"
cron = "*/10 * * * *"
"#,
    )
    .expect("write duplicate config");

    let state = load_startup_state(&workspace.workspace, &[]);
    let alert = state.config_alert.expect("config alert");

    assert_eq!(state.config.tasks.len(), 1);
    assert_eq!(state.config.tasks[0].name, "local-task");
    assert_eq!(alert.severity, ConfigIssueSeverity::Error);
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("Duplicate task name"))
    );
}

#[test]
fn reload_keeps_previous_config_when_referenced_sources_fail() {
    let workspace = make_workspace();
    fs::write(
        workspace.workspace.config_path(),
        r#"
[notifications]
enabled = false

[[task]]
name = "kept-task"
command = "echo hello"
cron = "*/5 * * * *"
"#,
    )
    .expect("write config");
    let current = load_startup_state(&workspace.workspace, &[]);

    fs::write(
        workspace.workspace.config_path(),
        r#"
[general]
task_sources = ["missing-source-dir"]

[notifications]
enabled = false

[[task]]
name = "new-task"
command = "echo hello"
cron = "*/5 * * * *"
"#,
    )
    .expect("write broken config");

    let outcome = load_reload_state(&workspace.workspace, &[], &current);
    let alert = outcome.state.config_alert.expect("config alert");

    assert!(!outcome.applied);
    assert_eq!(outcome.state.config.tasks[0].name, "kept-task");
    assert_eq!(alert.severity, ConfigIssueSeverity::Error);
    assert!(alert.recovery.contains("kept the previous config"));
    assert!(
        alert
            .issues
            .iter()
            .any(|issue| issue.message.contains("does not exist"))
    );
}
