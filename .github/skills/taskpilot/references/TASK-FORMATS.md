# TaskPilot Task Definition Formats

TaskPilot can load task definitions from the main `config.toml` and from external directories.
External `.toml` files support two formats.

## Multi-Task Format

A file containing a `[[task]]` array — same syntax as `config.toml`:

```toml
[[task]]
name = "team-backup"
command = "robocopy C:\\Shared D:\\Backup /MIR"
cron = "0 3 * * *"
timeout = "15m"

[[task]]
name = "team-cleanup"
command = "del /q C:\\Temp\\*.tmp"
cron = "0 4 * * *"
```

## Single-Task Format

A flat file with the task fields at the top level (no `[[task]]` wrapper):

```toml
name = "nightly-report"
command = "python generate_report.py"
cron = "0 23 * * 1-5"
timeout = "10m"
working_dir = "C:\\Scripts"
run_missed = true
```

### Single-task with custom shell

```toml
name = "ps-cleanup"
command = "Remove-Item $env:TEMP\\*.tmp -Force -ErrorAction SilentlyContinue"
cron = "0 4 * * *"
shell = "pwsh"
timeout = "5m"
```

### Single-task with triggers

```toml
name = "build"
command = "cargo build --release"
cron = "0 8 * * *"
triggers = [
  { task = "test", on = "success" },
  { task = "notify-failure", on = "failure" },
]
```

### Trigger-only task (no cron)

Tasks that are only triggered by other tasks can omit the `cron` field:

```toml
name = "deploy"
command = "deploy.ps1"
```

TaskPilot tries multi-task parsing first. If no `[[task]]` array is found, it falls back to
single-task parsing.

---

## External Task Source Directories

### Specifying sources

**In config.toml:**

```toml
[general]
task_sources = ["C:\\SharedTasks", "D:\\team-tasks"]
```

**On the command line:**

```bat
taskpilot.exe --task-dir C:\SharedTasks --task-dir D:\team-tasks
```

CLI and config sources are merged (duplicates removed by path).

### Behavior

- Only `.toml` files in the directory root are scanned (not recursive).
- Files that fail to parse are skipped with a warning.
- **File watching**: External directories are watched for changes. When `.toml` files are
  added, modified, or deleted, tasks reload automatically without restarting TaskPilot.
- External tasks appear in the dashboard with a 📁 source badge.
- External tasks are **read-only** in the dashboard.

---

## Individual Task Config Files

Instead of scanning an entire directory, you can point to specific `.toml` files.

### Specifying files

**In config.toml:**

```toml
[general]
task_configs = ["C:\\SharedTasks\\nightly-backup.toml", "~/my-task.toml"]
```

### Behavior

- Each file is loaded using the same multi-task / single-task parsing as directory sources.
- Non-existent files are skipped with a warning (TaskPilot still starts).
- Files that fail to parse are skipped with a warning.
- **File watching**: Individual files are watched for changes and reload automatically.
- Tasks from `task_configs` appear as external (📁 badge, read-only) in the dashboard.
- `task_sources` and `task_configs` can be used together.

### Name uniqueness

Task names must be globally unique across all sources. If a task name appears in more than one
source (local config or any external directory), TaskPilot logs an error and **falls back to
local tasks only** — external sources are skipped. The app still starts.

---

## Cron Expression Reference

TaskPilot uses standard 5-field cron expressions, evaluated in **local system time**:

```
┌───────────── minute (0–59)
│ ┌───────────── hour (0–23)
│ │ ┌───────────── day of month (1–31)
│ │ │ ┌───────────── month (1–12)
│ │ │ │ ┌───────────── day of week (0–6, Sunday = 0)
│ │ │ │ │
* * * * *
```

### Operators

| Operator | Meaning | Example |
|---|---|---|
| `*` | Any value | `* * * * *` — every minute |
| `,` | List | `0,30 * * * *` — at minute 0 and 30 |
| `-` | Range | `0 9-17 * * *` — every hour from 9 AM to 5 PM |
| `/` | Step | `*/5 * * * *` — every 5 minutes |

### Common Examples

| Expression | Meaning |
|---|---|
| `*/5 * * * *` | Every 5 minutes |
| `0 * * * *` | Every hour on the hour |
| `0 9 * * 1-5` | 9:00 AM, Monday–Friday |
| `30 2 * * *` | 2:30 AM every day |
| `0 0 1 * *` | Midnight on the 1st of every month |
| `0 0 * * 0` | Midnight every Sunday |
| `0 8,12,18 * * *` | 8 AM, noon, and 6 PM daily |
| `0 0 1,15 * *` | Midnight on the 1st and 15th of every month |

### Note on 6-field cron

Internally, TaskPilot converts 5-field expressions to 6-field by prepending `0` for the seconds
field. You should always write 5-field expressions in your config.
