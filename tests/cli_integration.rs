mod common;

use common::TestWorkspace;
use predicates::str::contains;

// ─── Test 1: No arguments → usage message ──────────────────────────────────

#[test]
fn no_args_shows_usage() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .assert()
        .failure()
        .stderr(contains("Usage:"));
}

// ─── Test 2: --list with basic config ───────────────────────────────────────

#[test]
fn list_shows_all_tasks() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .arg("--list")
        .assert()
        .success()
        .stdout(contains("echo-hello"))
        .stdout(contains("echo-date"));
}

// ─── Test 3: --run passing task ─────────────────────────────────────────────

#[test]
fn run_passing_task() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .args(["--run", "echo-hello"])
        .assert()
        .success()
        .stdout(contains("Hello from TaskPilot!"))
        .stdout(contains("passed"));
}

// ─── Test 4: --run failing task ─────────────────────────────────────────────

#[test]
fn run_failing_task() {
    let ws = TestWorkspace::from_fixture("config_failing.toml");
    ws.cli_cmd()
        .args(["--run", "fail-task"])
        .assert()
        .failure()
        .stderr(contains("failed"));
}

// ─── Test 5: --run nonexistent task ─────────────────────────────────────────

#[test]
fn run_nonexistent_task() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .args(["--run", "no-such-task"])
        .assert()
        .failure()
        .stderr(contains("task 'no-such-task' not found"));
}

// ─── Test 6: --run with timeout ─────────────────────────────────────────────

#[test]
fn run_task_with_timeout() {
    let ws = TestWorkspace::from_fixture("config_timeout.toml");
    ws.cli_cmd()
        .args(["--run", "slow-task"])
        .assert()
        .failure()
        .code(124)
        .stderr(contains("timed out"));
}

// ─── Test 7: --run with retries (still fails) ──────────────────────────────

#[test]
fn run_task_with_retries_still_fails() {
    let ws = TestWorkspace::from_fixture("config_retries.toml");
    ws.cli_cmd()
        .args(["--run", "retry-task"])
        .assert()
        .failure()
        .stderr(contains("failed"));
}

// ─── Test 8: --task-dir + --list shows external tasks ───────────────────────

#[test]
fn list_with_external_task_dir() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    let external_dir = common::fixtures_dir().join("external_tasks");

    ws.cli_cmd()
        .args(["--task-dir", external_dir.to_str().unwrap(), "--list"])
        .assert()
        .success()
        .stdout(contains("echo-hello"))
        .stdout(contains("greet"))
        .stdout(contains("batch-one"))
        .stdout(contains("batch-two"));
}

// ─── Test 9: --task-dir + --run external task ───────────────────────────────

#[test]
fn run_external_task() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    let external_dir = common::fixtures_dir().join("external_tasks");

    ws.cli_cmd()
        .args([
            "--task-dir",
            external_dir.to_str().unwrap(),
            "--run",
            "greet",
        ])
        .assert()
        .success()
        .stdout(contains("Greetings from external task!"));
}

// ─── Test 10: Workspace artifacts created after run ─────────────────────────

#[test]
fn workspace_artifacts_created() {
    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .args(["--run", "echo-hello"])
        .assert()
        .success();

    let runs_dir = ws.runs_dir().join("echo-hello");
    assert!(runs_dir.exists(), "Task runs directory should exist");

    let json_files: Vec<_> = std::fs::read_dir(&runs_dir)
        .expect("Failed to read runs dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "json")
        })
        .collect();

    assert!(
        !json_files.is_empty(),
        "At least one JSON run log should exist"
    );
}

// ─── Test 11: --run with working_dir ────────────────────────────────────────

#[test]
fn run_task_with_working_dir() {
    let tmp = tempfile::TempDir::new().expect("Failed to create working dir");
    let work_dir = tmp.path().to_string_lossy().to_string();

    let config = format!(
        r#"[general]
log_level = "info"

[notifications]
enabled = false

[[task]]
name = "dir-task"
command = "cd"
cron = "*/5 * * * *"
timeout = "10s"
notify_on_failure = false
working_dir = {working_dir}
"#,
        working_dir = toml::Value::String(work_dir.clone())
    );

    let ws = TestWorkspace::from_config_str(&config);

    // Extract the unique temp dir name for matching — Windows `cd` may return
    // 8.3 short paths (e.g. RMOREI~2) so we can't compare the full path.
    let dir_name = tmp
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    ws.cli_cmd()
        .args(["--run", "dir-task"])
        .assert()
        .success()
        .stdout(contains(&dir_name));
}

// ─── Test 12: Commands with TOML escape characters ──────────────────────────

#[test]
fn run_task_with_quoted_command() {
    let ws = TestWorkspace::from_fixture("config_escape.toml");

    // Single-quoted TOML string preserves inner double quotes literally
    ws.cli_cmd()
        .args(["--run", "echo-with-quotes"])
        .assert()
        .success()
        .stdout(contains("Hello from TaskPilot!"));
}

#[test]
fn run_task_with_backslash_path_in_command() {
    let ws = TestWorkspace::from_fixture("config_escape.toml");

    // Double-quoted TOML string: \\ becomes \ in the actual command
    ws.cli_cmd()
        .args(["--run", "echo-backslash-path"])
        .assert()
        .success()
        .stdout(contains("C:\\Users\\test\\scripts"));
}

#[test]
fn list_tasks_from_escape_config() {
    let ws = TestWorkspace::from_fixture("config_escape.toml");

    // Both tasks should load successfully despite special characters
    ws.cli_cmd()
        .arg("--list")
        .assert()
        .success()
        .stdout(contains("echo-with-quotes"))
        .stdout(contains("echo-backslash-path"));
}
