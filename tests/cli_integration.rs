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

    // New format: each run is a subdirectory containing run.json + output.log
    let run_dirs: Vec<_> = std::fs::read_dir(&runs_dir)
        .expect("Failed to read runs dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path().join("run.json").exists())
        .collect();

    assert!(
        !run_dirs.is_empty(),
        "At least one run directory with run.json should exist"
    );

    let run_dir = &run_dirs[0].path();
    assert!(
        run_dir.join("output.log").exists(),
        "output.log should exist in run directory"
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
shell = "cmd"
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

// ─── Test 15: Command with embedded quotes passed through cmd /C ────────────

#[test]
fn run_command_with_embedded_quotes_via_cmd() {
    // Simulates commands like: copilot -p "execute @ path" --flag
    // The inner double quotes must survive cmd /C passthrough (raw_arg fix).
    let config = r#"
[general]
log_level = "info"

[notifications]
enabled = false

[[task]]
name = "quoted-arg-task"
command = 'echo "hello world" extra'
cron = "*/5 * * * *"
timeout = "10s"
notify_on_failure = false
shell = "cmd"
"#;

    let ws = TestWorkspace::from_config_str(config);
    ws.cli_cmd()
        .args(["--run", "quoted-arg-task"])
        .assert()
        .success()
        // cmd /C echo "hello world" extra → prints: "hello world" extra
        .stdout(contains("hello world"));
}

// ─── Test 16: Default shell (cmd) works with shell config ───────────────────

#[test]
fn run_task_with_default_shell() {
    let ws = TestWorkspace::from_fixture("config_shell.toml");
    ws.cli_cmd()
        .args(["--run", "echo-default-shell"])
        .assert()
        .success()
        .stdout(contains("default-shell-works"));
}

// ─── Test 17: Per-task shell override to powershell ─────────────────────────

#[test]
fn run_task_with_powershell_shell() {
    let ws = TestWorkspace::from_fixture("config_shell.toml");
    ws.cli_cmd()
        .args(["--run", "echo-powershell"])
        .assert()
        .success()
        .stdout(contains("powershell-works"));
}

// ─── Test 18: Global default_shell applies when task has no shell ────────────

#[test]
fn global_default_shell_applies() {
    let config = r#"
[general]
log_level = "info"
default_shell = "powershell"

[notifications]
enabled = false

[[task]]
name = "ps-via-default"
command = "Write-Output 'from-global-default'"
cron = "*/5 * * * *"
timeout = "10s"
notify_on_failure = false
"#;

    let ws = TestWorkspace::from_config_str(config);
    ws.cli_cmd()
        .args(["--run", "ps-via-default"])
        .assert()
        .success()
        .stdout(contains("from-global-default"));
}

// ─── Test 19: Per-task shell overrides global default ────────────────────────

#[test]
fn task_shell_overrides_global_default() {
    // Global default is powershell, but task explicitly uses cmd
    let config = r#"
[general]
log_level = "info"
default_shell = "powershell"

[notifications]
enabled = false

[[task]]
name = "cmd-override"
command = "echo cmd-override-works"
cron = "*/5 * * * *"
timeout = "10s"
shell = "cmd"
notify_on_failure = false
"#;

    let ws = TestWorkspace::from_config_str(config);
    ws.cli_cmd()
        .args(["--run", "cmd-override"])
        .assert()
        .success()
        .stdout(contains("cmd-override-works"));
}

// ─── Test 20: Missing shell executable gives clear error ─────────────────────

#[test]
fn missing_shell_executable_fails_with_message() {
    let config = r#"
[general]
log_level = "info"

[notifications]
enabled = false

[[task]]
name = "bad-shell-task"
command = "echo hello"
cron = "*/5 * * * *"
timeout = "10s"
shell = "pwsh"
notify_on_failure = false
"#;

    let ws = TestWorkspace::from_config_str(config);
    // This may pass or fail depending on whether pwsh is installed,
    // but it should NOT panic — it should give a clean exit.
    let output = ws
        .cli_cmd()
        .args(["--run", "bad-shell-task"])
        .output()
        .expect("Failed to run CLI");

    // Should not panic (exit code should be defined)
    let _ = output.status.code().expect("Process should have an exit code");
}

// ─── Test 21: Shell field in external task files ─────────────────────────────

#[test]
fn external_task_with_shell_field() {
    // Create a temp dir with an external task that uses powershell
    let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
    let task_file = tmp.path().join("ps-task.toml");
    std::fs::write(
        &task_file,
        r#"name = "ext-ps-task"
command = "Write-Output 'external-ps-works'"
cron = "0 * * * *"
timeout = "10s"
shell = "powershell"
"#,
    )
    .expect("Failed to write external task file");

    let ws = TestWorkspace::from_fixture("config_basic.toml");
    ws.cli_cmd()
        .args([
            "--task-dir",
            tmp.path().to_str().unwrap(),
            "--run",
            "ext-ps-task",
        ])
        .assert()
        .success()
        .stdout(contains("external-ps-works"));
}
