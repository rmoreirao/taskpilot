# Copilot Instructions — TaskPilot

## Build & Deploy

```bat
cargo build --release          # release binary at target\release\taskpilot.exe
.\deploy.ps1                   # build + copy to D:\apps\taskpilot\
.\deploy.ps1 -DeployDir "X:\"  # custom deploy location
```

There is no test suite yet. No linter configuration exists beyond default `cargo check`.

## Architecture Overview

TaskPilot is a single-binary Windows GUI app (Rust + eframe/egui) that schedules shell commands via cron expressions and runs quietly in the system tray.

### Runtime flow

1. **Startup** (`main.rs`): Parse CLI args (`--minimized`, `--task-dir`), resolve workspace at `<exe_dir>/.taskpilot/` (fallback: `cwd`), load config, merge local + external task sources, launch eframe.
2. **Scheduler thread** (`scheduler.rs`): Runs in a dedicated thread, communicating with the UI via `mpsc` channels (`SchedulerCommand` / `SchedulerEvent`). Sleeps 1s per tick, checks cron expressions, spawns task threads. Persists schedule state (`state.json`) *before* task execution starts.
3. **Executor** (`executor.rs`): Runs commands via `cmd /C` on Windows. Handles timeouts (poll every 100ms, kill on expiry) and retries (loop `0..=retries`, save only final attempt).
4. **UI** (`ui/`): egui dashboard with sidebar navigation between Tasks, Settings, and Notifications views. Polls scheduler events and auto-refreshes every 2s.
5. **Tray** (`tray.rs`): System tray icon with background listener threads. Quit from the tray menu calls `process::exit(0)` directly, bypassing graceful shutdown.

### Shared state boundaries

- `Arc<Workspace>` is the shared persistence layer across app, scheduler, and executor.
- Scheduler ↔ UI: `mpsc` channels (`cmd_tx` for commands, `evt_rx` for events).
- Tray ↔ UI: `TrayManager::check_event()` plus direct egui viewport manipulation from tray threads.
- Config reload path: disk → `AppConfig::load()` → `task_sources::load_all()` → `SchedulerCommand::UpdateConfig`.

### Module responsibilities

| Module | Role |
|---|---|
| `config.rs` | Config structs with serde defaults, TOML loading/saving |
| `scheduler.rs` | Cron parsing (prepends `"0"` seconds field), scheduling loop, single-instance enforcement via `Arc<Mutex<HashSet>>` |
| `executor.rs` | Command spawning, `parse_duration()` for `Ns`/`Nm`/`Nh` strings, retry loop |
| `workspace.rs` | File-based persistence: run logs as JSON under `runs/<task>/`, scheduler state as `state.json`, debug log |
| `task_sources.rs` | External `.toml` directory scanning, dual format support (multi-task `[[task]]` / single-task flat), file watching with debounced reload |
| `autostart.rs` | Windows registry `HKCU\...\Run` integration, non-Windows stubs |
| `tray.rs` | Tray icon + menu, background listener threads |
| `ui/` | `mod.rs` dispatches to `dashboard`, `task_detail`, `settings`, `sidebar` |

## Documentation to Update

When changing user-facing behavior (CLI flags, config fields, task formats, UI features), update all affected docs:

| Doc | Covers |
|---|---|
| `README.md` | Feature list, configuration reference, CLI usage, project structure |
| `.github/skills/taskpilot/SKILL.md` | Copilot skill guide: config examples, CLI flags, workspace layout |
| `.github/skills/taskpilot/references/CONFIG-REFERENCE.md` | Full field-by-field config reference |
| `.github/skills/taskpilot/references/TASK-FORMATS.md` | External task file format specs |
| `config.example.toml` | Annotated starter config shipped with deploys |

## Key Conventions

- **Task name uniqueness is a hard invariant** across all sources (local config + external dirs). Duplicates cause a load error.
- **`TaskOrigin`** tracks whether a task is `Local` or `External { dir }`. External tasks are read-only in the UI and shown with a 📁 badge. Source metadata is keyed by task name in a `HashMap<String, TaskSourceInfo>`.
- **Workspace resolution**: `current_exe()` parent dir, falling back to `current_dir()`. All runtime data lives under `.taskpilot/` relative to that.
- **Config uses serde defaults** extensively (`#[serde(default)]`). The task array uses `#[serde(rename = "task")]` so TOML entries are `[[task]]` not `[[tasks]]`.
- **Error handling**: functions return `Result<T, String>` — no custom error types. Errors in external source loading are non-fatal (falls back to local-only tasks with a warning).
- **Run log filenames** are sanitized: non-alphanumeric characters (except `-` and `_`) become `_`.
- **UI theme colors** are defined as constants in `ui/mod.rs` (`GREEN`, `RED`, `BLUE`, `YELLOW`, `MUTED`).
- **Platform-specific code** is gated with `#[cfg(windows)]` blocks, with stub implementations for other platforms in `autostart.rs` and `executor.rs`.
