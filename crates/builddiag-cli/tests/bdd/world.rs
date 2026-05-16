//! World struct for cucumber tests.
//!
//! The World holds all state needed during scenario execution,
//! including the temporary directory, command output, and workspace configuration.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Output;
use tempfile::TempDir;

/// Test world holding state for a single scenario.
#[derive(Debug, Default, cucumber::World)]
pub struct BuilddiagWorld {
    /// Temporary directory for the test workspace.
    pub temp_dir: Option<TempDir>,

    /// Last command output.
    pub last_output: Option<Output>,

    /// Whether a basic Rust workspace has been initialized.
    pub has_workspace: bool,

    /// MSRV configuration for the workspace.
    pub msrv: Option<MsrvConfig>,

    /// Toolchain configuration.
    pub toolchain: Option<ToolchainConfig>,

    /// Whether a checksums file exists.
    pub has_checksums: bool,

    /// Custom config file content.
    pub config_content: Option<String>,

    /// Override for defaults.out_dir from config.
    pub out_dir_override: Option<String>,

    /// Override for --artifacts-dir from CLI args.
    pub artifacts_dir_override: Option<String>,

    /// Explicit --out path if provided.
    pub explicit_out: Option<String>,

    /// Profile to use for the check command.
    pub profile: Option<String>,

    /// Additional CLI arguments.
    pub extra_args: Vec<String>,

    /// Additional crates in the workspace.
    pub additional_crates: Vec<String>,

    /// Optional override for workspace.package.edition.
    pub workspace_edition: Option<String>,

    /// Custom config file path (relative to workspace).
    pub config_path: Option<String>,

    /// Custom files to write to the workspace.
    pub custom_files: HashMap<String, String>,
}

/// MSRV configuration options.
#[derive(Debug, Clone, Default)]
pub struct MsrvConfig {
    /// The MSRV version string.
    pub version: String,
    /// Where the MSRV is defined.
    pub location: MsrvLocation,
}

/// Where MSRV is defined in the workspace.
#[derive(Debug, Clone, Default)]
pub enum MsrvLocation {
    /// In workspace.package.rust-version.
    #[default]
    WorkspacePackage,
    /// Only in individual crate Cargo.toml.
    CrateOnly,
    /// Not defined anywhere.
    None,
}

/// Toolchain configuration options.
#[derive(Debug, Clone, Default)]
pub struct ToolchainConfig {
    /// The channel specification.
    pub channel: String,
}

impl BuilddiagWorld {
    /// Get the path to the temporary directory.
    pub fn workspace_path(&self) -> PathBuf {
        self.temp_dir
            .as_ref()
            .expect("workspace not initialized")
            .path()
            .to_path_buf()
    }

    /// Get the exit code from the last command.
    pub fn exit_code(&self) -> i32 {
        self.last_output
            .as_ref()
            .expect("no command has been run")
            .status
            .code()
            .expect("process terminated by signal")
    }

    /// Get stdout from the last command.
    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(
            &self
                .last_output
                .as_ref()
                .expect("no command has been run")
                .stdout,
        )
        .to_string()
    }

    /// Get stderr from the last command.
    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(
            &self
                .last_output
                .as_ref()
                .expect("no command has been run")
                .stderr,
        )
        .to_string()
    }
}
