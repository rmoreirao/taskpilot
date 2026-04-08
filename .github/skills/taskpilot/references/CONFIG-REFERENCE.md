# TaskPilot Configuration Reference

Full field-by-field reference for `.taskpilot/config.toml`.

## `[general]`

| Field | Type | Default | Description |
|---|---|---|---|
| `log_level` | string | `"info"` | Log verbosity. One of: `"debug"`, `"info"`, `"warn"`, `"error"`. *(Parsed but not yet enforced at runtime — reserved for future use.)* |
| `max_log_retention_days` | integer | `30` | Days to keep task run log files before pruning. *(Parsed but not yet enforced at runtime — reserved for future use.)* |
| `start_with_windows` | boolean | `false` | Persisted state of the auto-start toggle. Changed via the **Settings** UI checkbox, which registers/unregisters the Windows Run registry key. This field is **not** applied automatically on startup or config reload. |
| `task_sources` | array of strings | `[]` | List of external directories containing `.toml` task definitions. Paths support `~/` expansion. Merged with CLI `--task-dir` arguments. |
| `task_configs` | array of strings | `[]` | List of individual `.toml` task definition files. Paths support `~/` expansion. Same format as files in `task_sources` directories. |

### Example

```toml
[general]
log_level = "info"
max_log_retention_days = 30
start_with_windows = true
task_sources = ["C:\\SharedTasks", "~/team-tasks"]
task_configs = ["C:\\SharedTasks\\special-task.toml", "~/my-task.toml"]
```

---

## `[notifications]`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | boolean | `true` | Master switch for all desktop notifications. |
| `on_failure` | boolean | `true` | Notify when a task exits with a non-zero code or times out. *(Parsed but not yet wired — currently, failure notifications are controlled by the per-task `notify_on_failure` field and the global `enabled` switch.)* |
| `on_recovery` | boolean | `true` | Notify when a previously-failed task succeeds again. *(Parsed but not yet wired — reserved for future use.)* |
| `sound` | boolean | `false` | Play a sound with notifications. *(Parsed but not yet wired — reserved for future use.)* |

### Example

```toml
[notifications]
enabled = true
on_failure = true
on_recovery = true
sound = false
```

---

## `[updates]`

| Field | Type | Default | Description |
|---|---|---|---|
| `auto_check` | boolean | `true` | Automatically check GitHub for new releases every 24 hours. When an update is found, the dashboard shows a banner and the Settings view offers one-click download and apply. |

### Example

```toml
[updates]
auto_check = true
```

---

## `[[task]]`

Each task is a repeatable `[[task]]` table. You can define as many as needed.

### Required Fields

| Field | Type | Description |
|---|---|---|
| `name` | string | Unique identifier shown in the dashboard. Must be unique across all sources (local + external). |
| `command` | string | Command to run. Executed via `cmd /C` on Windows, `sh -c` elsewhere. |
| `cron` | string | Standard 5-field cron expression: `minute hour day month weekday`. |

### Optional Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `timeout` | string | none | Maximum run time before the task is killed. Format: `"30s"`, `"5m"`, `"1h"`, or plain seconds `"60"`. |
| `working_dir` | string | none | Directory to run the command in. Supports `~/` expansion. |
| `notify_on_failure` | boolean | `true` | Override the global notification setting for this task. |
| `retries` | integer | `0` | Number of additional attempts if the task fails (exit code ≠ 0). The task is retried immediately. |

### Example

```toml
[[task]]
name = "nightly-backup"
command = "robocopy C:\\Data D:\\Backup /MIR"
cron = "0 2 * * *"
timeout = "10m"
working_dir = "C:\\Scripts"
notify_on_failure = true
retries = 2
```

---

## Timeout Format

The `timeout` field accepts the following suffixes:

| Suffix | Meaning | Example |
|---|---|---|
| `s` | Seconds | `"30s"` → 30 seconds |
| `m` | Minutes | `"5m"` → 5 minutes |
| `h` | Hours | `"1h"` → 1 hour |
| *(none)* | Seconds | `"60"` → 60 seconds |

If a task exceeds its timeout, the process is killed and the run status is set to `timeout`.

---

## Quoting & Escaping in Commands

Commands in TOML can use **double-quoted** or **single-quoted** strings. This matters when your command contains backslashes (Windows paths) or embedded quotes.

### The problem

TOML double-quoted strings treat `\` as an escape character. Only specific sequences like `\n`, `\t`, `\\`, and `\"` are valid. Writing a raw Windows path like `"echo C:\Data"` causes a **parse error** because `\D` is not a valid TOML escape.

### Solution: use single-quoted strings

TOML single-quoted strings are **literal** — no escape processing. Backslashes and double quotes are kept as-is:

```toml
# ✅ Correct — single quotes, backslashes are literal
command = 'robocopy C:\Data D:\Backup /MIR'

# ✅ Correct — embedded double quotes work too
command = 'copilot -p "execute @ .github\prompts\my-task.prompt.md" --autopilot'
```

### Alternative: escape backslashes in double-quoted strings

If you prefer double quotes, double every backslash (`\\`):

```toml
# ✅ Correct — escaped backslashes in double quotes
command = "robocopy C:\\Data D:\\Backup /MIR"

# ❌ WRONG — \D and \B are invalid TOML escapes
command = "robocopy C:\Data D:\Backup /MIR"
```

### Quick reference

| TOML syntax | Backslash behavior | Best for |
|---|---|---|
| `'single quotes'` | Literal (no escaping) | Windows paths, commands with `"` |
| `"double quotes"` | `\\` → `\`, `\"` → `"` | Simple commands without backslashes |

### Real-world example

```toml
[[task]]
name = "run-copilot-prompt"
command = 'copilot -p "execute @ .github\prompts\echo-test.prompt.md" --allow-all --autopilot --model claude-sonnet-4.6'
cron = "0 7 * * *"
timeout = "1h"
```
