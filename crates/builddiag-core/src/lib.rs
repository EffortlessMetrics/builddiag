//! Public library API for builddiag build contract validation.
//!
//! This crate provides a stable, Clap-free entry point for running builddiag
//! checks as a library dependency. It wraps the internal `builddiag-app`
//! orchestration layer with a clean public API.
//!
//! # Quick Start
//!
//! ```ignore
//! use builddiag_core::{Settings, run};
//! use camino::Utf8PathBuf;
//!
//! let settings = Settings {
//!     root: Utf8PathBuf::from("."),
//!     ..Default::default()
//! };
//! let result = run(&settings)?;
//! println!("Verdict: {:?}", result.report.verdict);
//! ```
//!
//! # Architecture
//!
//! `builddiag-core` is a thin facade over `builddiag-app` that:
//! - Provides a unified [`run`] entry point producing both report formats
//! - Exposes a [`Settings`] struct for all configuration
//! - Re-exports key types from [`builddiag_types`]
//! - Has no dependency on `clap` or any CLI framework

use anyhow::Result;
#[cfg(feature = "cache")]
pub use builddiag_app::CacheConfig;
use builddiag_app::{
    load_config as app_load_config, run_check_with_sensor, run_check_with_sensor_from_repo_state,
};
use builddiag_repo::repo_state_from_substrate;
use builddiag_types::{CheckReport, Config, SensorReport, Substrate};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeSet;

// Re-exports of key types from builddiag-types for convenience.
pub use builddiag_types::{
    Artifact, Capability, Finding, ManifestInfo, Report, Severity, Substrate as SubstrateType,
    ToolInfo, Verdict, VerdictStatus,
};

/// Configuration for a builddiag run.
///
/// Collects all parameters needed to execute checks. All fields have
/// sensible defaults via [`Default`].
///
/// # Examples
///
/// ```
/// use builddiag_core::Settings;
/// use camino::Utf8PathBuf;
///
/// // Minimal: check current directory with defaults
/// let settings = Settings::default();
///
/// // Customised
/// let settings = Settings {
///     root: Utf8PathBuf::from("/path/to/repo"),
///     allow_all: true,
///     ..Default::default()
/// };
/// ```
pub struct Settings {
    /// Repository root to check.
    pub root: Utf8PathBuf,
    /// Configuration (loaded from TOML or defaults).
    pub config: Config,
    /// Run all checks even when diff-aware mode skips them.
    pub allow_all: bool,
    /// Files changed between git refs, for diff-aware mode.
    pub changed_files: Option<BTreeSet<String>>,
    /// Cache configuration (requires `cache` feature).
    #[cfg(feature = "cache")]
    pub cache_config: Option<CacheConfig>,
    /// Pre-computed repository state from an upstream tool.
    ///
    /// When set, builddiag skips disk-based repo discovery and uses
    /// the supplied substrate instead. This enables in-process
    /// integration where the caller has already parsed the workspace.
    pub substrate: Option<Substrate>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: Utf8PathBuf::from("."),
            config: Config::default(),
            allow_all: false,
            changed_files: None,
            #[cfg(feature = "cache")]
            cache_config: None,
            substrate: None,
        }
    }
}

/// Result of a [`run`] invocation.
///
/// Contains both the native builddiag report and the Cockpit CI sensor report,
/// plus rendered outputs and the computed exit code.
pub struct RunResult {
    /// Native builddiag.report.v1 report.
    pub report: Report,
    /// Cockpit-compatible sensor.report.v1 report.
    pub sensor_report: SensorReport,
    /// Rendered Markdown summary.
    pub markdown: String,
    /// GitHub Actions annotation lines.
    pub annotations: Vec<String>,
    /// Suggested process exit code (0 = ok, 2 = policy violation).
    pub exit_code: i32,
    /// Per-check reports (useful for downstream analysis).
    pub checks: Vec<CheckReport>,
}

/// Primary entry point — run all checks and produce both report formats.
///
/// Delegates to the internal orchestration layer, loading repo state,
/// executing checks, and building both the native and sensor reports.
///
/// # Errors
///
/// Returns an error if repository loading or check execution fails.
///
/// # Examples
///
/// ```ignore
/// use builddiag_core::{Settings, run};
///
/// let settings = Settings::default();
/// let result = run(&settings)?;
/// assert!(result.exit_code == 0 || result.exit_code == 2);
/// ```
pub fn run(settings: &Settings) -> Result<RunResult> {
    let sr = if let Some(ref substrate) = settings.substrate {
        // Substrate path: build RepoState from pre-computed data, skip disk I/O
        let repo_state = repo_state_from_substrate(&settings.root, substrate);
        run_check_with_sensor_from_repo_state(
            &settings.root,
            &settings.config,
            settings.allow_all,
            repo_state,
        )?
    } else {
        // Standard path: discover repo from disk
        run_check_with_sensor(
            &settings.root,
            &settings.config,
            settings.allow_all,
            settings.changed_files.clone(),
            #[cfg(feature = "cache")]
            settings.cache_config.as_ref(),
        )?
    };

    Ok(RunResult {
        report: sr.check_run.report,
        sensor_report: sr.sensor_report,
        markdown: sr.check_run.markdown,
        annotations: sr.check_run.annotations,
        exit_code: sr.check_run.exit_code,
        checks: sr.checks,
    })
}

/// Load configuration from an optional TOML path.
///
/// If `path` is `None`, returns the default configuration.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn load_config(path: Option<&Utf8Path>) -> Result<Config> {
    app_load_config(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_is_current_dir() {
        let s = Settings::default();
        assert_eq!(s.root, Utf8PathBuf::from("."));
        assert!(!s.allow_all);
        assert!(s.changed_files.is_none());
    }

    #[test]
    fn load_config_none_returns_defaults() {
        let cfg = load_config(None).unwrap();
        assert_eq!(cfg.defaults.out_dir, "artifacts/builddiag");
    }

    #[test]
    fn reexports_are_accessible() {
        // Compile-time check that re-exports work
        let _: Severity = Severity::Info;
        let _: Verdict = Verdict::Pass;
        let _: VerdictStatus = VerdictStatus::Pass;
    }

    #[test]
    fn substrate_settings_default_is_none() {
        let s = Settings::default();
        assert!(s.substrate.is_none());
    }

    #[test]
    fn run_with_substrate_produces_result() {
        use builddiag_types::{ManifestInfo, Substrate};

        // Use the valid-workspace fixture directory as root context
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures")
            .join("conformance")
            .join("valid-workspace");
        let root = camino::Utf8PathBuf::from_path_buf(fixture_dir).unwrap();

        if !root.exists() {
            return; // Skip if fixture not available
        }

        let substrate = Substrate {
            manifests: vec![ManifestInfo {
                path: "Cargo.toml".to_string(),
                name: Some("valid-workspace".to_string()),
                msrv: Some("1.75".to_string()),
                edition: Some("2024".to_string()),
            }],
            has_toolchain: true,
            toolchain_channel: Some("1.75.0".to_string()),
            has_checksums: false,
            has_lockfile: false,
            workspace_msrv: Some("1.75".to_string()),
        };

        let settings = Settings {
            root,
            config: builddiag_types::Config {
                profile: builddiag_types::Profile::Oss,
                ..Default::default()
            },
            substrate: Some(substrate),
            ..Default::default()
        };

        let result = run(&settings).expect("run with substrate should succeed");
        // Should produce a valid report
        assert_eq!(result.report.schema, "builddiag.report.v1");
        assert!(!result.sensor_report.schema.is_empty());
    }
}
