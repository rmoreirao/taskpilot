---
name: taskpilot
description: >
  Guide for configuring and interacting with TaskPilot, a lightweight Windows task scheduler.
  Use this skill when asked to create, edit, or troubleshoot TaskPilot configuration files (config.toml),
  add or modify scheduled tasks, configure notifications, set up external task source directories,
  run or test tasks from the command line, or launch TaskPilot from the command line.
  Trigger keywords include: taskpilot, config.toml, cron task, task scheduler, scheduled command,
  task source, --task-dir, --minimized, --run, --list.
compatibility: >
  Windows. TaskPilot is a native Windows application. Commands are executed via cmd /C by default,
  with optional PowerShell (powershell/pwsh), sh, or bash shells configurable per-task or globally.
  Requires the taskpilot.exe (GUI) and/or taskpilot-cli.exe (CLI) binaries.
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
command = 'robocopy C:\Data D:\Backup /MIR'
cron = "0 2 * * *"
timeout = "10m"
notify_on_failure = true
```

> **Tip:** Use single-quoted TOML strings (`'...'`) for commands with Windows paths or embedded
> quotes — backslashes are treated literally. In double-quoted strings, you must escape them: `"C:\\Data"`.

### 2. Launch TaskPilot

```bat
taskpilot.exe
```

The app opens a dashboard window and a system tray icon. Closing the window hides it to the tray.

### 3. Reload after config changes

After editing `config.toml`, open the **Settings** view in the dashboard and click **Reload Config**.
No restart is needed. External task source directories are watched automatically and reload on file changes.

## CLI Flags

### GUI binary (`taskpilot.exe`)

| Flag | Description |
|---|---|
| `--minimized` | Start without showing the window (useful for auto-start at login) |
| `--task-dir <path>` | Load additional task definitions from `<path>`. Repeatable. |
| `--renderer auto\|wgpu\|glow` | Choose rendering backend. `auto` (default) probes for a GPU and falls back to OpenGL. Use `glow` on GPU-less servers. |
| `--version` | Print the version and exit |

### CLI binary (`taskpilot-cli.exe`)

| Flag | Description |
|---|---|
| `--list` | Print all configured task names with their source and cron schedule |
| `--run <name>` | Execute a task immediately, print output, and exit with the task's exit code |
| `--task-dir <path>` | Load additional task definitions from `<path>`. Repeatable. |
| `--check-update` | Check GitHub for a newer release and print the result |
| `--update` | Download and apply the latest release, then exit |
| `--version` | Print the version and exit |

Examples:

```bat
rem Launch hidden, loading tasks from two shared directories
taskpilot.exe --minimized --task-dir C:\SharedTasks --task-dir D:\team-tasks

rem List all configured tasks
taskpilot-cli --list

rem Run a specific task and see its output
taskpilot-cli --run my-backup

rem Test an external task
taskpilot-cli --task-dir C:\SharedTasks --run team-cleanup

rem Check for updates
taskpilot-cli --check-update

rem Download and apply the latest update
taskpilot-cli --update
```

The CLI binary shares the same `.taskpilot/` workspace and config as the GUI.
It runs tasks synchronously, saves run results to the workspace (visible in the GUI's
run history), and exits with the task's exit code (0 = passed, non-zero = failed, 124 = timeout).

CLI `--task-dir` paths are merged with `task_sources` from `config.toml` (duplicates removed).

## Workspace Layout

TaskPilot stores all runtime data in `.taskpilot/` next to the executable:

```
.taskpilot/
├── config.toml          # Main configuration file
├── state.json           # Scheduler state (last/next run times, local time)
├── update-state.json    # Auto-update state (last check, available version)
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
| `[updates]` | Auto-update preferences (check frequency, enable/disable) |
| `[[task]]` | Repeatable — one entry per scheduled task |

For the full field-by-field reference, see [CONFIG-REFERENCE.md](references/CONFIG-REFERENCE.md).

## Adding a Task

Each `[[task]]` entry requires three fields:

```toml
[[task]]
name = "health-check"          # Unique identifier
command = "curl http://localhost:8080/health"   # Executed via cmd /C (default)
cron = "*/5 * * * *"           # Standard 5-field cron expression (local time)
```

Optional fields: `timeout` (e.g. `"30s"`, `"5m"`, `"1h"`), `working_dir`, `notify_on_failure`, `retries`, `run_missed` (default: `true` — catch up overdue tasks on startup/resume; set `false` to skip), `shell` (override: `"cmd"`, `"powershell"`, `"pwsh"`, `"sh"`, `"bash"`).

#### Shell override

```toml
# Global default (in [general]):
[general]
default_shell = "pwsh"

# Per-task override:
[[task]]
name = "ps-report"
command = "Get-Process | Out-File C:\\logs\\procs.txt"
cron = "0 8 * * *"
shell = "pwsh"
```

PowerShell note: non-terminating errors exit 0 by default. Use `$ErrorActionPreference = 'Stop'` or `exit 1` to ensure TaskPilot detects failures.

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
- **"OpenGL 2.0+" or "no suitable adapter" errors on Windows Server?** The machine likely has no
  GPU. Try `--renderer glow` (uses OpenGL) or `--renderer wgpu` (uses DirectX). In `auto` mode
  (the default), TaskPilot probes for a GPU and falls back automatically. If both renderers fail,
  install GPU drivers, enable GPU redirection in Remote Desktop, or place a Mesa3D `opengl32.dll`
  next to `taskpilot.exe` for software OpenGL.

## Building from Source

```bat
cargo build --release
```

The binary is at `target\release\taskpilot.exe` (GUI) and `target\release\taskpilot-cli.exe` (CLI). Use `deploy.ps1` to build and copy both to a target directory:

```powershell
.\deploy.ps1                              # default: D:\apps\taskpilot\
.\deploy.ps1 -DeployDir "C:\MyApps\tp"    # custom location
```

The deploy script never overwrites an existing `config.toml`.

## Starter Config Template

A fully annotated example config is available at [assets/config.example.toml](assets/config.example.toml).
