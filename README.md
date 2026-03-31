# TaskPilot

A lightweight desktop task scheduler with system tray integration, built with Rust and [egui](https://github.com/emilk/egui).

TaskPilot runs scheduled jobs (cron-based), displays their status in a dashboard UI, and lives in your system tray for easy access.

## Features

- **Cron-based scheduling** — Define jobs with standard cron expressions
- **System tray integration** — Runs in the background; click the tray icon to open
- **Dashboard UI** — View all jobs, their status, success rates, and execution history
- **Notifications** — Desktop notifications on job failure or recovery
- **Job retries & timeouts** — Configurable per job
- **Auto-start with Windows** — Optional startup registration

## Building

### Prerequisites

- [Rust](https://rustup.rs/) (2021 edition)
- On Windows: MSVC build tools (Visual Studio)

### Build

```sh
cargo build
```

### Run

```sh
cargo run
```

To start minimized to the system tray:

```sh
cargo run -- --minimized
```

## Configuration

TaskPilot looks for `.taskpilot/config.toml` in the current working directory. A sample config is provided in `config.example.toml`.

### Example config

```toml
[general]
log_level = "info"
max_log_retention_days = 30
start_with_windows = false

[notifications]
enabled = true
on_failure = true
on_recovery = true
sound = false

[[jobs]]
name = "example-hello"
command = "echo Hello from TaskPilot!"
cron = "*/5 * * * *"
timeout = "30s"
notify_on_failure = true

[[jobs]]
name = "example-date"
command = "date /t"
cron = "*/2 * * * *"
timeout = "10s"
notify_on_failure = true
```

### Job configuration fields

| Field              | Required | Description                                      |
|--------------------|----------|--------------------------------------------------|
| `name`             | Yes      | Unique job name                                  |
| `command`          | Yes      | Shell command to execute                         |
| `cron`             | Yes      | Cron expression for scheduling                   |
| `timeout`          | No       | Max execution time (e.g., `30s`, `5m`, `1h`)     |
| `working_dir`      | No       | Working directory for the command                |
| `notify_on_failure`| No       | Send notification on failure (default: `true`)   |
| `retries`          | No       | Number of retry attempts on failure              |

## License

MIT
