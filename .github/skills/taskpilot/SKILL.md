---
name: taskpilot
description: >
  Guide for configuring and interacting with TaskPilot, a lightweight Windows task scheduler.
  Use this skill when asked to create, edit, or troubleshoot TaskPilot configuration files (config.toml),
  add or modify scheduled tasks, configure notifications, set up external task source directories,
  or launch TaskPilot from the command line. Trigger keywords include: taskpilot, config.toml,
  cron task, task scheduler, scheduled command, task source, --task-dir, --minimized.
compatibility: >
  Windows. TaskPilot is a native Windows application. Commands are executed via cmd /C.
  Requires the taskpilot.exe binary.
---

# TaskPilot — Configuration & CLI Skill

TaskPilot is a lightweight Windows task scheduler. You configure it with a single TOML file and
interact with it via CLI flags or the built-in GUI dashboard.

## Quick Start

### 1. Create or edit the config file

TaskPilot reads `.taskpilot/config.toml` from the directory where `taskpilot.exe` lives.
If the file doesn't exist on first launch, a starter config is created automatically.

```toml
[general]
log_level = "info"
start_with_windows = false

[notifications]
enabled = true
on_failure = true

[[task]]
name = "my-backup"
command = "robocopy C:\\Data D:\\Backup /MIR"
cron = "0 2 * * *"
timeout = "10m"
notify_on_failure = true
```

### 2. Launch TaskPilot

```bat
taskpilot.exe
```

The app opens a dashboard window and a system tray icon. Closing the window hides it to the tray.

### 3. Reload after config changes

After editing `config.toml`, open the **Settings** view in the dashboard and click **Reload Config**.
No restart is needed. External task source directories are watched automatically and reload on file changes.

## CLI Flags

| Flag | Description |
|---|---|
| `--minimized` | Start without showing the window (useful for auto-start at login) |
| `--task-dir <path>` | Load additional task definitions from `<path>`. Repeatable. |

Examples:

```bat
rem Launch hidden, loading tasks from two shared directories
taskpilot.exe --minimized --task-dir C:\SharedTasks --task-dir D:\team-tasks
```

CLI `--task-dir` paths are merged with `task_sources` from `config.toml` (duplicates removed).

## Workspace Layout

TaskPilot stores all runtime data in `.taskpilot/` next to the executable:

```
.taskpilot/
├── config.toml          # Main configuration file
├── state.json           # Scheduler state (last/next run times)
├── runs/                # Task run history
│   └── <task-name>/     # One directory per task
│       └── YYYY-MM-DDTHHMMSS.json   # Individual run results
└── debug/
    └── app.log          # Debug log
```

## Config Sections Overview

| Section | Purpose |
|---|---|
| `[general]` | App-level settings: log level, retention, auto-start, external task sources |
| `[notifications]` | Desktop notification preferences |
| `[[task]]` | Repeatable — one entry per scheduled task |

For the full field-by-field reference, see [CONFIG-REFERENCE.md](references/CONFIG-REFERENCE.md).

## Adding a Task

Each `[[task]]` entry requires three fields:

```toml
[[task]]
name = "health-check"          # Unique identifier
command = "curl http://localhost:8080/health"   # Executed via cmd /C
cron = "*/5 * * * *"           # Standard 5-field cron expression
```

Optional fields: `timeout` (e.g. `"30s"`, `"5m"`, `"1h"`), `working_dir`, `notify_on_failure`, `retries`.

## External Task Sources

You can load tasks from directories outside the main config. This is useful for shared team
task libraries or repo-specific task definitions.

```toml
[general]
task_sources = ["C:\\SharedTasks", "D:\\team-tasks"]
```

Each directory is scanned for `.toml` files in two supported formats.
See [TASK-FORMATS.md](references/TASK-FORMATS.md) for details.

External directories are **watched for changes** — when `.toml` files are added, modified, or deleted,
tasks reload automatically. External tasks appear in the dashboard with a 📁 badge and are read-only.

Task names must be unique across all sources. If duplicates are found, TaskPilot logs an error
and falls back to local tasks only (external sources are skipped). The app still starts.

## Common Cron Expressions

| Expression | Meaning |
|---|---|
| `*/5 * * * *` | Every 5 minutes |
| `0 * * * *` | Every hour on the hour |
| `0 9 * * 1-5` | 9:00 AM, Monday–Friday |
| `30 2 * * *` | 2:30 AM daily |
| `0 0 1 * *` | Midnight on the 1st of every month |

## Auto-Start with Windows

Toggle the **Start with Windows** checkbox in the **Settings** view to register/unregister
TaskPilot in the Windows `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` registry key.
The `start_with_windows` field in `config.toml` reflects the persisted state of this toggle —
it is **not** applied automatically on startup or config reload; use the UI to change it.

## Troubleshooting

- **Only example tasks showing?** If `config.toml` fails to parse, TaskPilot silently falls back
  to default example tasks. Check the file for TOML syntax errors.
- **External tasks not loading?** If any task name collides across sources, all external sources
  are skipped. Check `debug/app.log` for error messages.
- **Config changes not taking effect?** Click **Reload Config** in the Settings view. External
  source directories are watched automatically, but `config.toml` itself requires a manual reload.

## Building from Source

```bat
cargo build --release
```

The binary is at `target\release\taskpilot.exe`. Use `deploy.ps1` to build and copy to a target directory:

```powershell
.\deploy.ps1                              # default: D:\apps\taskpilot\
.\deploy.ps1 -DeployDir "C:\MyApps\tp"    # custom location
```

The deploy script never overwrites an existing `config.toml`.

## Starter Config Template

A fully annotated example config is available at [assets/config.example.toml](assets/config.example.toml).
