use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const GITHUB_REPO: &str = "rmoreirao/taskpilot";
const GITHUB_API_BASE: &str = "https://api.github.com";

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Persisted update state stored in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateState {
    pub last_check: Option<DateTime<Local>>,
    pub available_version: Option<String>,
    pub release_url: Option<String>,
    pub download_url_gui: Option<String>,
    pub download_url_cli: Option<String>,
}

impl UpdateState {
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize update state: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write update state: {}", e))
    }

    pub fn has_update(&self) -> bool {
        self.available_version.is_some()
    }

    pub fn needs_check(&self, interval_hours: u64) -> bool {
        match self.last_check {
            Some(last) => {
                let elapsed = Local::now().signed_duration_since(last);
                elapsed.num_hours() >= interval_hours as i64
            }
            None => true,
        }
    }

    pub fn clear_update(&mut self) {
        self.available_version = None;
        self.release_url = None;
        self.download_url_gui = None;
        self.download_url_cli = None;
    }
}

/// Information about a GitHub release.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub html_url: String,
    pub gui_asset_url: Option<String>,
    pub cli_asset_url: Option<String>,
}

/// Check the GitHub API for the newest release by semver. Returns `None` if no releases exist.
pub fn check_latest_release() -> Result<Option<ReleaseInfo>, String> {
    let url = format!(
        "{}/repos/{}/releases?per_page=30",
        GITHUB_API_BASE, GITHUB_REPO
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("TaskPilot/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("Failed to fetch release info: {}", e))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!(
            "GitHub API returned status {}",
            response.status()
        ));
    }

    let releases: Vec<serde_json::Value> = response
        .json()
        .map_err(|e| format!("Failed to parse releases JSON: {}", e))?;

    // Find the release with the highest semver version, skipping drafts and prereleases
    let mut best: Option<(semver::Version, &serde_json::Value)> = None;

    for release in &releases {
        if release["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        if release["prerelease"].as_bool().unwrap_or(false) {
            continue;
        }

        let tag = match release["tag_name"].as_str() {
            Some(t) => t,
            None => continue,
        };
        let ver_str = tag.strip_prefix('v').unwrap_or(tag);
        let ver = match semver::Version::parse(ver_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if best.as_ref().map_or(true, |(best_ver, _)| ver > *best_ver) {
            best = Some((ver, release));
        }
    }

    let (_version, body) = match best {
        Some(b) => b,
        None => return Ok(None),
    };

    let tag_name = body["tag_name"].as_str().unwrap_or("").to_string();
    let html_url = body["html_url"].as_str().unwrap_or("").to_string();
    let version = tag_name.strip_prefix('v').unwrap_or(&tag_name).to_string();

    // Find download URLs for the exe assets
    let mut gui_asset_url = None;
    let mut cli_asset_url = None;

    if let Some(assets) = body["assets"].as_array() {
        for asset in assets {
            let name = asset["name"].as_str().unwrap_or("");
            let download_url = asset["browser_download_url"].as_str().unwrap_or("");

            if name == "taskpilot-cli.exe" {
                cli_asset_url = Some(download_url.to_string());
            } else if name == "taskpilot.exe" {
                gui_asset_url = Some(download_url.to_string());
            }
        }
    }

    Ok(Some(ReleaseInfo {
        version,
        tag_name,
        html_url,
        gui_asset_url,
        cli_asset_url,
    }))
}

/// Compare two semver version strings. Returns true if `remote` is newer than `local`.
pub fn is_newer_version(local: &str, remote: &str) -> bool {
    let local_ver = match semver::Version::parse(local) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let remote_ver = match semver::Version::parse(remote) {
        Ok(v) => v,
        Err(_) => return false,
    };
    remote_ver > local_ver
}

/// Check for updates and return the new state.
pub fn check_for_update() -> Result<UpdateState, String> {
    let mut state = UpdateState {
        last_check: Some(Local::now()),
        ..Default::default()
    };

    let release = match check_latest_release()? {
        Some(r) => r,
        None => return Ok(state), // No releases exist yet — not an error
    };

    if is_newer_version(CURRENT_VERSION, &release.version) {
        state.available_version = Some(release.version);
        state.release_url = Some(release.html_url);
        state.download_url_gui = release.gui_asset_url;
        state.download_url_cli = release.cli_asset_url;
    }

    Ok(state)
}

/// Download a file from a URL to a local path.
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("TaskPilot/{}", CURRENT_VERSION))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to download {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read download body: {}", e))?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    std::fs::write(dest, &bytes)
        .map_err(|e| format!("Failed to write file: {}", e))
}

/// Result of an update download + apply operation.
#[derive(Debug)]
pub struct UpdateResult {
    pub version: String,
    pub gui_updated: bool,
    pub cli_updated: bool,
    pub needs_restart: bool,
}

/// Download and apply an update. Returns info about what was updated.
///
/// Strategy for Windows:
///   1. Download new exe to `<name>.exe.new`
///   2. Rename running `<name>.exe` to `<name>.exe.old`
///   3. Rename `<name>.exe.new` to `<name>.exe`
///   4. On next startup, `cleanup_old_binaries()` deletes `.old` files
pub fn download_and_apply(state: &UpdateState) -> Result<UpdateResult, String> {
    let version = state
        .available_version
        .as_ref()
        .ok_or("No update available")?
        .clone();

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .ok_or("Cannot determine executable directory")?;

    let mut gui_updated = false;
    let mut cli_updated = false;

    // Update the GUI binary
    if let Some(gui_url) = &state.download_url_gui {
        let gui_path = exe_dir.join("taskpilot.exe");
        apply_binary_update(gui_url, &gui_path)?;
        gui_updated = true;
    }

    // Update the CLI binary
    if let Some(cli_url) = &state.download_url_cli {
        let cli_path = exe_dir.join("taskpilot-cli.exe");
        apply_binary_update(cli_url, &cli_path)?;
        cli_updated = true;
    }

    Ok(UpdateResult {
        version,
        gui_updated,
        cli_updated,
        needs_restart: gui_updated,
    })
}

/// Apply a single binary update using the rename strategy.
fn apply_binary_update(url: &str, target_path: &Path) -> Result<(), String> {
    let new_path = target_path.with_extension("exe.new");
    let old_path = target_path.with_extension("exe.old");

    // Download to .new
    download_file(url, &new_path)?;

    // Rename current to .old (if it exists)
    if target_path.exists() {
        // Remove previous .old if it exists
        if old_path.exists() {
            let _ = std::fs::remove_file(&old_path);
        }
        std::fs::rename(target_path, &old_path).map_err(|e| {
            // Clean up the .new file on failure
            let _ = std::fs::remove_file(&new_path);
            format!(
                "Failed to rename {} to {}: {}",
                target_path.display(),
                old_path.display(),
                e
            )
        })?;
    }

    // Rename .new to target
    std::fs::rename(&new_path, target_path).map_err(|e| {
        // Try to restore the old binary on failure
        if old_path.exists() {
            let _ = std::fs::rename(&old_path, target_path);
        }
        let _ = std::fs::remove_file(&new_path);
        format!(
            "Failed to rename {} to {}: {}",
            new_path.display(),
            target_path.display(),
            e
        )
    })
}

/// Remove `.old` files left from a previous update.
pub fn cleanup_old_binaries() {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for name in &["taskpilot.exe.old", "taskpilot-cli.exe.old"] {
                let old_path = exe_dir.join(name);
                if old_path.exists() {
                    let _ = std::fs::remove_file(&old_path);
                }
            }
        }
    }
}

/// Get the path to the update state file in the workspace.
pub fn update_state_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("update-state.json")
}
