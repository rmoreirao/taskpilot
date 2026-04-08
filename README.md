# TaskPilot

> **A lightweight task scheduler for Windows** — schedule scripts and commands with cron expressions, monitor them from a clean dashboard, and let it run quietly in your system tray.

[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://github.com/rmoreirao/taskpilot/releases)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange)](https://www.rust-lang.org/)

---

## What is TaskPilot?

TaskPilot is a simple, no-fuss task scheduler for Windows. If you have scripts, batch files, or commands that need to run on a schedule — backups, data syncs, cleanup routines, health checks — TaskPilot gets out of your way and just runs them.

**Why not Windows Task Scheduler?**

| | Windows Task Scheduler | TaskPilot |
|---|---|---|
| Configuration | XML / GUI wizard | One plain-text TOML file |
| Scheduling syntax | Complex trigger system | Standard cron expressions |
| Monitoring | Event Viewer | Built-in dashboard |
| Setup time | 10+ clicks | Edit one file, launch app |
| Footprint | System service | Single `.exe` in your tray |

---

## Features

- **Cron scheduling** — Use familiar `*/5 * * * *` syntax to define when tasks run
- **Minimize to tray** — Closing the window hides it to the system tray; click the tray icon to restore
- **Live dashboard** — See every task's status, last run time, exit code, and stdout/stderr at a glance
- **CLI mode** — Run or test any task from the command line with `taskpilot-cli --run <name>`
- **Desktop notifications** — Get a Windows notification when a task fails or recovers
- **Retries & timeouts** — Configure per-task retry counts and maximum run durations
- **Run history** — Drill into any task to browse a full log of past executions
- **Auto-start with Windows** — Optionally register TaskPilot to launch at login
- **Auto-update** — Checks GitHub for new releases every 24 hours; download and apply with one click from the dashboard or via `taskpilot-cli --update`

---

## Installation

### Option 1 — Download release (recommended)

1. Go to the [Releases page](https://github.com/rmoreirao/taskpilot/releases) and download `taskpilot.exe`.
2. Place it anywhere on your machine (e.g. `C:\Tools\taskpilot\taskpilot.exe`).
3. Double-click to run. TaskPilot will create a `.taskpilot\` folder next to the executable with a starter config.

### Option 2 — Build from source

See [Building from Source](#building-from-source) below.

---

## Quick Start

1. **Run TaskPilot.** A tray icon appears in the system tray.
2. **Open the dashboard** by clicking the tray icon.
3. **Edit the config file** at `.taskpilot\config.toml` (in the same folder as `taskpilot.exe`).
4. Add your first task:

```toml
[[task]]
name = "my-backup"
command = 'robocopy C:\Data D:\Backup /MIR'
cron = "0 2 * * *"        # runs every day at 2:00 AM
timeout = "10m"
notify_on_failure = true
```

5. Click **Reload Config** in the Settings view — your task is now scheduled.

---

## Configuration Reference

TaskPilot reads `.taskpilot\config.toml` from the directory where `taskpilot.exe` lives.  
A fully annotated example is provided in [`config.example.toml`](config.example.toml).

### `[general]`

| Field | Default | Description |
|---|---|---|
| `log_level` | `"info"` | Log verbosity: `"debug"`, `"info"`, `"warn"`, `"error"` |
| `max_log_retention_days` | `30` | Days to keep task run logs before pruning |
| `start_with_windows` | `false` | Register TaskPilot to launch automatically at login |
| `task_sources` | `[]` | List of external directories containing `.toml` task definitions |

### `[notifications]`

| Field | Default | Description |
|---|---|---|
| `enabled` | `true` | Master switch for all desktop notifications |
| `on_failure` | `true` | Notify when a task exits with a non-zero code or times out |
| `on_recovery` | `true` | Notify when a previously-failed task succeeds again |
| `sound` | `false` | Play a sound with notifications |

### `[updates]`

| Field | Default | Description |
|---|---|---|
| `auto_check` | `true` | Check GitHub for new releases every 24 hours |

When an update is found, the dashboard shows a banner and the **Settings** view lets you download and apply with one click. You can also update from the command line with `taskpilot-cli --update`.

### `[[task]]`

Each task is a `[[task]]` table entry. You can define as many as you like.

| Field | Required | Description |
|---|---|---|
| `name` | ✅ | Unique identifier shown in the dashboard |
| `command` | ✅ | Command to run (executed via `cmd /C`). Use single-quoted TOML strings for commands with backslashes — see [Quoting & Escaping](#quoting--escaping-in-commands). |
| `cron` | ✅ | 5-field cron expression (`minute hour day month weekday`) |
| `timeout` | — | Maximum run time before the task is killed (e.g. `30s`, `5m`, `1h`) |
| `working_dir` | — | Directory to run the command in (supports `~/` expansion) |
| `notify_on_failure` | — | Override the global notification setting for this task (default: `true`) |
| `retries` | — | Number of additional attempts if the task fails (default: `0`) |

#### Cron expression examples

| Expression | Meaning |
|---|---|
| `*/5 * * * *` | Every 5 minutes |
| `0 * * * *` | Every hour on the hour |
| `0 9 * * 1-5` | 9:00 AM, Monday–Friday |
| `30 2 * * *` | 2:30 AM every day |
| `0 0 1 * *` | Midnight on the 1st of every month |

#### Quoting & Escaping in commands

TOML double-quoted strings treat `\` as an escape character — `\D`, `\B`, `\p` etc. are **invalid** and will cause a config parse error. Use **single-quoted** TOML strings for commands with Windows paths or embedded quotes:

```toml
# ✅ Single quotes — backslashes and inner quotes are literal
command = 'robocopy C:\Data D:\Backup /MIR'
command = 'copilot -p "execute @ .github\prompts\my-task.prompt.md" --autopilot'

# ✅ Double quotes — but you must escape backslashes with \\
command = "robocopy C:\\Data D:\\Backup /MIR"

# ❌ WRONG — \D is not a valid TOML escape sequence
command = "robocopy C:\Data D:\Backup /MIR"
```

### External Task Sources

TaskPilot can load tasks from external directories in addition to the local `config.toml`. This is useful for shared team task libraries, centrally managed definitions, or separating task definitions across repos.

External directories are scanned for `.toml` files. Each file can use either format:

**Multi-task format** (same as `config.toml`):

```toml
[[task]]
name = "team-backup"
command = 'robocopy C:\Shared D:\Backup /MIR'
cron = "0 3 * * *"

[[task]]
name = "team-cleanup"
command = 'del /q C:\Temp\*.tmp'
cron = "0 4 * * *"
```

**Single-task format** (one task per file):

```toml
name = "nightly-report"
command = "python generate_report.py"
cron = "0 23 * * 1-5"
timeout = "10m"
```

**Specifying sources** — use `task_sources` in config and/or `--task-dir` on the command line:

```toml
[general]
task_sources = ["C:\\SharedTasks", "D:\\team-tasks"]
```

```bat
taskpilot.exe --task-dir C:\SharedTasks --task-dir D:\team-tasks
```

CLI and config sources are merged (duplicates removed). If any task name appears in more than one source, TaskPilot reports an error.

External directories are **watched for changes** — when `.toml` files are added, modified, or deleted, tasks are automatically reloaded. External tasks are shown in the dashboard with a 📁 source badge and are read-only.

---

## Usage

### Launch normally

```bat
taskpilot.exe
```

The main window opens. Closing the window hides it to the system tray (it does not quit the app).

### Launch minimized (recommended for auto-start)

```bat
taskpilot.exe --minimized
```

Starts without showing the window — useful when auto-starting with Windows.

### Load tasks from external directories

```bat
taskpilot.exe --task-dir C:\SharedTasks --task-dir D:\team-tasks
```

Loads additional task definitions from the specified directories. Can be combined with `--minimized`. See [External Task Sources](#external-task-sources) for details.

### CLI mode — Run or test a task from the terminal

A companion `taskpilot-cli.exe` is provided for command-line use. It shares the same config and workspace as the GUI but runs in the console with full stdout/stderr support.

```bat
:: List all configured tasks
taskpilot-cli --list

:: Run a specific task immediately and see its output
taskpilot-cli --run my-backup

:: Run with additional external task sources
taskpilot-cli --task-dir C:\SharedTasks --run team-cleanup

:: Check if a newer version is available
taskpilot-cli --check-update

:: Download and apply the latest update
taskpilot-cli --update
```

The CLI binary:
- Loads the same `.taskpilot\config.toml` and external task sources as the GUI
- Executes the task synchronously and streams stdout/stderr to the terminal
- Saves run results to the workspace (visible in the GUI's run history)
- Exits with the task's exit code (0 = passed, non-zero = failed, 124 = timeout)

### Tray icon

| Action | Result |
|---|---|
| Left-click | Open / restore the dashboard window |
| Right-click → Quit | Exit TaskPilot completely |

### Reload config

After editing `config.toml`, open the **Settings** view in the dashboard and click **Reload Config**. No restart needed.

---

## Workspace & Logs

TaskPilot stores all runtime data in `.taskpilot/` next to the executable:

```
.taskpilot/
├── config.toml                        # Main configuration file
├── state.json                         # Scheduler state (last/next run times)
├── update-state.json                  # Auto-update state (last check, available version)
├── runs/                              # Task run history
│   └── <task-name>/                   # One directory per task
│       └── YYYY-MM-DDTHHMMSS.json    # Individual run result
└── debug/
    └── app.log                        # Debug log (append-only)
```

Each **run result** is a JSON file containing `task_name`, `status`, `exit_code`, `stdout`, `stderr`, `started_at`, `finished_at`, and `duration_ms`. Tasks execute silently in the background — commands are run via `cmd /C` with stdout/stderr captured in-memory (no terminal window is opened).

The **debug log** is an append-only text file with lines in the format `[ISO-8601 timestamp] [component] message`.

---

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2021, stable toolchain)
- Windows with MSVC build tools ([Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/))

### Steps

```bat
git clone https://github.com/rmoreirao/taskpilot.git
cd taskpilot
cargo build --release
```

The binary will be at `target\release\taskpilot.exe` (GUI) and `target\release\taskpilot-cli.exe` (CLI).

### Deploy to a target folder

A convenience script is provided to build and deploy the exe to a target directory (default `D:\apps\taskpilot\`):

```powershell
.\deploy.ps1                              # deploy to default location
.\deploy.ps1 -DeployDir "C:\MyApps\tp"    # deploy to a custom location
```

The script:
- Builds in release mode
- Copies `taskpilot.exe` and `taskpilot-cli.exe` to the target directory
- **Never overwrites** an existing `config.toml`, preserving your settings

---

## Project Structure

```
taskpilot/
├── src/
│   ├── lib.rs            # Shared library (modules used by both binaries)
│   ├── main.rs           # GUI entry point (windows_subsystem = "windows")
│   ├── cli.rs            # CLI entry point (console subsystem)
│   ├── app.rs            # App state and main event loop
│   ├── config.rs         # Config structs and TOML loading
│   ├── scheduler.rs      # Cron-based task scheduling loop
│   ├── executor.rs       # Command execution, timeouts, retries
│   ├── workspace.rs      # Run logs and scheduler state persistence
│   ├── tray.rs           # System tray icon and menu
│   ├── autostart.rs      # Windows startup registration
│   ├── task_sources.rs   # External task directory loading
│   ├── updater.rs        # Auto-update via GitHub releases
│   └── ui/               # egui dashboard, sidebar, settings
├── assets/               # App icon (embedded at build time)
├── config.example.toml   # Annotated example configuration
├── deploy.ps1            # Build & deploy script
└── Cargo.toml
```

---

## License

MIT — see [LICENSE](LICENSE) for details.
