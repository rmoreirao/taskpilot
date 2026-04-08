use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Path to the fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Set up an isolated test workspace by copying a fixture config into a temp dir.
/// Returns the TempDir (workspace root = `<tmp>/.taskpilot/`) and the workspace path.
pub struct TestWorkspace {
    pub _tmp: TempDir,
    pub workspace_dir: PathBuf,
}

impl TestWorkspace {
    /// Create a workspace from a fixture config file.
    pub fn from_fixture(fixture_name: &str) -> Self {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let workspace_dir = tmp.path().join(".taskpilot");
        fs::create_dir_all(&workspace_dir).expect("Failed to create workspace dir");

        let fixture_path = fixtures_dir().join(fixture_name);
        let config_dest = workspace_dir.join("config.toml");
        fs::copy(&fixture_path, &config_dest).unwrap_or_else(|e| {
            panic!(
                "Failed to copy fixture {} to {}: {}",
                fixture_path.display(),
                config_dest.display(),
                e
            )
        });

        Self {
            _tmp: tmp,
            workspace_dir,
        }
    }

    /// Create a workspace from a config string (for dynamic configs).
    pub fn from_config_str(config: &str) -> Self {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let workspace_dir = tmp.path().join(".taskpilot");
        fs::create_dir_all(&workspace_dir).expect("Failed to create workspace dir");

        let config_dest = workspace_dir.join("config.toml");
        fs::write(&config_dest, config).expect("Failed to write config");

        Self {
            _tmp: tmp,
            workspace_dir,
        }
    }

    /// Get a Command pre-configured with the workspace env var.
    pub fn cli_cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("taskpilot-cli").expect("Failed to find taskpilot-cli");
        cmd.env("TASKPILOT_WORKSPACE", &self.workspace_dir);
        cmd
    }

    /// Path to the runs directory inside the workspace.
    pub fn runs_dir(&self) -> PathBuf {
        self.workspace_dir.join("runs")
    }
}
