# Continuous Operation Options

TaskPilot currently runs as a GUI app tied to the user's login session. This document evaluates strategies for running tasks continuously — even when the user is logged out — while preserving access to user-profile tools (git, az cli, gh cli, Copilot CLI) that depend on user credentials and tokens.

## The Fundamental Triangle

Every approach must choose a trade-off between three properties:

1. **Runs while logged out** — tasks execute on schedule regardless of login state
2. **Uses real user profile/tokens** — access to `%USERPROFILE%`, `%APPDATA%`, Credential Manager, SSH keys
3. **Renews tokens unattended** — can handle interactive re-auth (browser flows, MFA prompts)

**No single solution achieves all three.** Each option below picks a different compromise.

### User Profile Access by Runtime Context

| Runtime context                                   | User profile? | Credential Manager? | User env vars? | Interactive auth? |
| ------------------------------------------------- | ------------- | -------------------- | --------------- | ----------------- |
| **Logged-in session (today)**                     | ✅            | ✅                   | ✅              | ✅                |
| **Task Scheduler "run whether logged on or not"** | ✅            | ⚠️ partial            | ✅              | ❌                |
| **Service as your user account**                  | ✅            | ⚠️ partial            | ✅              | ❌                |
| **Service as SYSTEM**                             | ❌            | ❌                   | ❌              | ❌                |

> **Note:** "Partial" Credential Manager means file-based tokens (e.g. `~/.config/gh/`) work, but session-only persistence entries do not.

---

## Tier 1 — Pragmatic & Low Complexity

### A. Catch-Up Missed Runs on Login

On startup, compare each task's `next_run` against the current time. If overdue, execute immediately.

- **Gets you:** All 3 properties while logged in; graceful handling of gaps
- **Loses:** On-time execution while logged out (tasks run late)
- **Complexity:** Low — scheduler logic change only
- **Best for:** Most personal-machine use cases; this is how `anacron` and macOS `launchd` work

### B. Disconnected RDP Session

Log in (locally or via RDP), start TaskPilot, then disconnect without logging off. The Windows session stays alive in the background.

- **Gets you:** Full user session — all tools and tokens work normally
- **Loses:** Fragile; breaks on reboot, group policy, or accidental logoff
- **Complexity:** Zero code changes — operational pattern only
- **Best for:** Power users who want a quick "keep it running" trick

### C. Auto-Login + Immediate Lock

Configure Windows to auto-login at boot, start TaskPilot minimized, then immediately lock the workstation.

- **Gets you:** Persistent real user session with full profile/token access
- **Loses:** Security — auto-login stores credentials; physical access risk
- **Complexity:** Low (installer/config, not code changes)
- **Best for:** Dedicated machines, home labs, kiosk-style setups

### D. Automation Identities

Replace human tokens with long-lived automation credentials:

- **git:** SSH keys or Personal Access Tokens (PATs)
- **gh:** `GH_TOKEN` environment variable
- **az:** Service principal (`az login --service-principal`)
- **APIs:** Direct API calls with stored secrets

Properties:

- **Gets you:** Runs logged out + user-profile access (partially)
- **Loses:** "As the human" identity; audit trail differs; Copilot CLI awkward
- **Complexity:** Low (per-tool configuration, no code changes)
- **Best for:** Tasks where the identity doesn't matter, just the access

---

## Tier 2 — Architectural (Medium Complexity)

### E. Service + Delegate to Windows Task Scheduler

TaskPilot runs as a service (the scheduling brain). When a task is due, it creates a one-shot Windows Scheduled Task set to "Run whether user is logged on or not" under the user's account.

- **Gets you:** Always-on scheduling + user profile access
- **Loses:** Browser re-auth; password stored by Windows Task Scheduler
- **Complexity:** Medium
- **Best for:** Pragmatic always-on without reimplementing credential management

### F. Service + User-Agent Broker

Split TaskPilot into two processes:

- **Service** (always-on): scheduler, state management, retries, logs
- **User agent** (starts at login): owns user tokens, executes credentialed tasks

Communication via named pipes or local HTTP. Service runs headless tasks anytime; credentialed tasks route through the user agent when available, queue otherwise.

- **Gets you:** Best coverage — headless tasks run 24/7, credentialed tasks run when possible
- **Loses:** Credentialed tasks still queue until login
- **Complexity:** Medium-High (IPC layer, two processes to deploy)
- **Best for:** Cleanest long-term architecture; mirrors how CI runners work

### G. Task Capability Tagging

Tasks declare whether they need user context:

```toml
[[task]]
name = "backup-repo"
command = "git pull"
requires_user_session = true

[[task]]
name = "cleanup-temp"
command = "del /q %TEMP%\\*.tmp"
requires_user_session = false
```

Non-credentialed tasks run via service; credentialed ones catch-up on login.

- **Gets you:** Partial always-on with clear expectations
- **Loses:** Not all tasks run 24/7
- **Complexity:** Medium (new task field, routing logic)
- **Best for:** Incremental step toward the full service + agent architecture

---

## Tier 3 — Power User / Infrastructure

### H. Remote Runner Offload

Dispatch eligible tasks to external compute:

- GitHub Actions (via `workflow_dispatch` API)
- Azure Automation / Functions
- A cloud VM or self-hosted runner

Properties:

- **Gets you:** True 24/7 execution independent of local machine
- **Loses:** Local resources (mapped drives, local repos); needs secret management
- **Complexity:** Medium-High (API integration, artifact sync)
- **Best for:** Cloud-native tasks (deployments, repo maintenance, API calls)

### I. Dedicated Always-On VM

Run TaskPilot on a Hyper-V/cloud Windows VM permanently logged in under a user account. Access via RDP when needed.

- **Gets you:** All 3 properties (real session on a machine that never logs off)
- **Loses:** Cost; another machine to manage; credential duplication
- **Complexity:** Low code, Medium ops
- **Best for:** Users who already have infrastructure; bot hosts

### J. Service + `LogonUser` / `CreateProcessAsUser`

TaskPilot service creates a fresh Windows logon session per task run using Win32 APIs (`LogonUser` + `LoadUserProfile` + `CreateProcessAsUser`).

- **Gets you:** User profile, HKCU registry, home directory access while logged out
- **Loses:** Must store user password securely; no browser/MFA reauth; complex Win32 edge cases
- **Complexity:** High (Win32 token/profile APIs are fiddly)
- **Best for:** Advanced users willing to manage stored credentials

### K. Credential Broker

A lightweight user-mode agent owns sensitive tokens and exposes a narrow RPC interface (named pipe or local HTTP): "get Azure token", "run git pull as me", etc. The always-on service calls the broker for credentialed operations.

- **Gets you:** Clean separation of concerns; service never touches raw credentials
- **Loses:** Broker must be running (same login dependency)
- **Complexity:** Medium-High
- **Best for:** Architecturally clean credential isolation

---

## Tier 4 — Creative / Niche

| Approach                            | Summary                                                        | Verdict                                           |
| ----------------------------------- | -------------------------------------------------------------- | ------------------------------------------------- |
| **WSL / container sidecar**         | Scheduler in WSL systemd, bridge to Windows creds              | Partial; Windows tokens don't map cleanly         |
| **Token-refresh daemon**            | Proactively renew OAuth tokens before expiry                   | Fragile, vendor-specific, security risk           |
| **Browser automation / RPA reauth** | Script browser flows to renew tokens unattended                | Extremely fragile; last resort only               |
| **Kerberos / gMSA / CredSSP**      | Domain delegation primitives for service-to-user impersonation | Enterprise-only; doesn't help consumer CLI tokens |

---

## What CI Runners Actually Do

Production CI systems (GitHub Actions runners, Azure DevOps agents, Jenkins nodes) converge on the same pattern:

1. **Service process** handles scheduling, queuing, and lifecycle
2. **Worker process** runs in user context for execution
3. **Automation identities** (service principals, PATs, SSH keys) instead of human tokens
4. **Accept that some things can't run unattended** — interactive auth is a human concern

---

## Recommendation for TaskPilot

An incremental approach that avoids a big-bang architecture change:

### Phase 1 — Catch-Up Missed Runs (Option A) ✅

Implemented. On each scheduler tick, tasks with `next_run` in the past are classified:
- **`run_missed = true` (default):** executed as a catch-up run with `⏰` notification
- **`run_missed = false`:** skipped, `next_run` advanced, `⏭️` notification shown
- Cron expression changes while app was down are detected via persisted `cron_expr` in state
- `last_status` is now persisted on task completion
- Works for both cold start and resume-from-sleep scenarios

### Phase 2 — Task Capability Tagging (Option G)

Add a `requires_user_session` field to the task config. This lays the groundwork for routing tasks differently based on their needs and sets clear expectations for users.

### Phase 3 — Service + User Agent (Option F)

For headless tasks, build an always-on service scheduler. Credentialed tasks continue to route through the user-session agent. This is the long-term architecture.

### Supported Deployment Patterns (Document Now)

Document auto-login + lock (Option C) and disconnected RDP (Option B) as supported deployment patterns for power users who need always-on operation today without waiting for the service architecture.
