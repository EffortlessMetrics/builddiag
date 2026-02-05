//! Shared types for the builddiag build contract validator.
//!
//! This crate provides the core data structures used throughout builddiag:
//!
//! - [`Report`] - The main output structure containing all check results
//! - [`Config`] - Configuration schema for customizing check behavior
//! - [`Finding`] - Individual validation findings with severity and location
//! - [`Location`] - File location information for findings
//! - [`Severity`] - Finding severity levels (Info, Warn, Error)
//! - [`Verdict`] - Overall verdict (Pass, Warn, Fail, Error)
//!
//! # Report Structure (builddiag.report.v1)
//!
//! The [`Report`] type is the primary output of builddiag. It contains:
//! - Schema identifier for versioning
//! - Tool information (name, version) (optional)
//! - Run metadata (timestamps, duration, host, git info) (optional)
//! - Overall verdict
//! - Flattened list of findings from all checks
//! - Optional summary with counts by severity and check
//! - Optional report-level data for downstream tooling
//!
//! # Configuration
//!
//! The [`Config`] type controls how builddiag behaves:
//! - Default settings for output and failure conditions
//! - File paths for input files
//! - Policy settings for MSRV, toolchain, and checksums
//! - Per-check overrides
//!
//! # Example
//!
//! ```
//! use builddiag_types::{Finding, Severity, Location};
//!
//! let finding = Finding {
//!     check_id: "rust.msrv_defined".to_string(),
//!     code: "missing_msrv".to_string(),
//!     severity: Severity::Error,
//!     message: "Missing rust-version in Cargo.toml".to_string(),
//!     location: Some(Location {
//!         path: "Cargo.toml".to_string(),
//!         line: Some(1),
//!         col: None,
//!     }),
//! };
//! ```

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// =============================================================================
// New Report Schema Types (builddiag.report.v1)
// =============================================================================

/// Information about the host machine where builddiag was executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostInfo {
    /// Operating system name (e.g., "linux", "macos", "windows").
    pub os: String,
    /// CPU architecture (e.g., "x86_64", "aarch64").
    pub arch: String,
}

/// Git repository information at the time of the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitInfo {
    /// Current commit SHA.
    pub commit: String,
    /// Current branch name, if on a branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether the working directory has uncommitted changes.
    pub dirty: bool,
}

/// File location information for a finding.
///
/// Paths are repo-relative and use forward slashes on all platforms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    /// Repository-relative file path (always uses forward slashes).
    pub path: String,
    /// Line number in the file (1-indexed), if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number in the file (1-indexed), if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

/// Summary statistics for the new report format.
///
/// Provides aggregated counts of findings by severity and check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReportSummary {
    /// Total number of findings across all checks.
    pub total_findings: usize,
    /// Count of findings by severity level (keys: "info", "warn", "error").
    pub by_severity: BTreeMap<String, usize>,
    /// Count of findings by check ID.
    pub by_check: BTreeMap<String, usize>,
}

// =============================================================================
// Legacy Types (kept for backward compatibility)
// =============================================================================

/// Identifier for the JSON schema version used in reports.
///
/// This allows consumers to verify they can parse the report format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchemaId(pub String);

/// Information about the builddiag tool that generated the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolInfo {
    /// Name of the tool (e.g., "builddiag").
    pub name: String,
    /// Version of the tool (e.g., "0.1.0").
    pub version: String,
}

/// Information about a single execution run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunInfo {
    /// Timestamp when the run started (ISO 8601 format).
    pub started_at: DateTime<Utc>,
    /// Timestamp when the run completed, if finished (ISO 8601 format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// Duration of the run in milliseconds.
    pub duration_ms: u64,
    /// Information about the host machine.
    pub host: HostInfo,
    /// Git repository information, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
}

/// Information about the detected repository structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepoDetected {
    /// Whether the repository is a Cargo workspace.
    pub is_workspace: bool,
    /// Number of workspace members (1 for single-crate projects).
    pub members: usize,
}

/// Information about the repository being analyzed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepoInfo {
    /// Root path of the repository.
    pub root: String,
    /// Detected repository structure.
    pub detected: RepoDetected,
}

/// Paths to input files that were analyzed.
///
/// These paths are relative to the repository root. A `None` value
/// indicates the file was not found or not applicable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Inputs {
    /// Path to the root Cargo.toml file.
    pub cargo_root: Option<String>,
    /// Path to the rust-toolchain.toml file.
    pub rust_toolchain: Option<String>,
    /// Path to the tools checksums file.
    pub tools_checksums: Option<String>,
    /// Path to the tools manifest file.
    pub tools_manifest: Option<String>,
}

/// Severity level of a finding.
///
/// Severity determines how a finding affects the overall verdict:
/// - `Info` - Informational, does not affect verdict
/// - `Warn` - Warning, may affect verdict depending on `fail_on` setting
/// - `Error` - Error, causes failure unless `fail_on` is set to `Never`
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational finding that does not indicate a problem.
    Info,
    /// Warning that may indicate a potential issue.
    Warn,
    /// Error that indicates a definite problem.
    Error,
}

/// Status of a check execution.
///
/// Each check produces a status indicating its result:
/// - `Pass` - Check passed with no issues
/// - `Warn` - Check passed but with warnings
/// - `Fail` - Check failed
/// - `Skip` - Check was skipped (e.g., missing required input)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Check passed successfully.
    Pass,
    /// Check passed with warnings.
    Warn,
    /// Check failed.
    Fail,
    /// Check was skipped.
    Skip,
}

/// Overall verdict for the report.
///
/// The verdict summarizes all check results into a single outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// All checks passed.
    Pass,
    /// Some checks produced warnings.
    Warn,
    /// One or more checks failed.
    Fail,
    /// All checks were skipped.
    Skip,
    /// An internal error occurred during execution.
    Error,
}

/// A single validation finding from a check.
///
/// Findings represent individual issues or observations discovered during
/// validation. Each finding has a severity, a code for programmatic handling,
/// and a human-readable message.
///
/// # Examples
///
/// ```
/// use builddiag_types::{Finding, Severity, Location};
///
/// let finding = Finding {
///     check_id: "rust.msrv_defined".to_string(),
///     code: "missing_msrv".to_string(),
///     severity: Severity::Error,
///     message: "Missing rust-version in Cargo.toml".to_string(),
///     location: Some(Location {
///         path: "Cargo.toml".to_string(),
///         line: Some(1),
///         col: None,
///     }),
///
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// Identifier of the check that produced this finding.
    pub check_id: String,
    /// Machine-readable code identifying the type of finding.
    pub code: String,
    /// Severity level of this finding.
    pub severity: Severity,
    /// Human-readable description of the finding.
    pub message: String,
    /// File location where the finding was detected, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// Results from executing a single check.
///
/// Each check produces a report containing its status and any findings.
/// If the check was skipped, `skipped_reason` explains why.
///
/// Note: This is retained for backward compatibility during migration.
/// New code should use findings directly in the Report struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CheckReport {
    /// Unique identifier for this check (e.g., "msrv_defined").
    pub id: String,
    /// Overall status of the check execution.
    pub status: CheckStatus,
    /// Individual findings from this check.
    pub findings: Vec<Finding>,
    /// Reason the check was skipped, if `status` is `Skip`.
    pub skipped_reason: Option<String>,
}

/// Counts of findings by severity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummaryCounts {
    /// Number of informational findings.
    pub info: usize,
    /// Number of warning findings.
    pub warn: usize,
    /// Number of error findings.
    pub error: usize,
}

/// Summary of all check results.
///
/// The summary aggregates findings across all checks and provides
/// counts by severity and check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    /// Total number of findings across all checks.
    pub total_findings: usize,
    /// Count of findings by severity level (keys: "info", "warn", "error").
    pub by_severity: BTreeMap<String, usize>,
    /// Count of findings by check ID.
    pub by_check: BTreeMap<String, usize>,
}

/// The main report output from builddiag.
///
/// A report contains all information about a validation run, including
/// metadata, findings, and summary statistics.
///
/// # Schema Version
///
/// This report follows the `builddiag.report.v1` schema. The `schema` field
/// is always set to "builddiag.report.v1" for this version.
///
/// # Structure
///
/// - `schema` - Schema version identifier (const "builddiag.report.v1")
/// - `tool` - Information about the builddiag version (optional)
/// - `run` - Execution metadata (timestamps, duration, host, git) (optional)
/// - `verdict` - Overall verdict (Pass, Warn, Fail, Error)
/// - `findings` - Flattened list of all findings from all checks
/// - `summary` - Optional aggregated statistics
/// - `data` - Optional report-level data for downstream tooling
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    /// Schema identifier for this report format.
    /// Always "builddiag.report.v1" for this version.
    pub schema: String,
    /// Information about the tool that generated this report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    /// Information about this execution run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunInfo>,
    /// Overall verdict based on all findings.
    pub verdict: Verdict,
    /// All findings from all checks, flattened into a single list.
    pub findings: Vec<Finding>,
    /// Summary statistics, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    /// Optional arbitrary report-level metadata for downstream tooling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Report {
    /// The schema identifier for builddiag.report.v1.
    pub const SCHEMA_V1: &'static str = "builddiag.report.v1";
}

// -----------------
// Config
// -----------------

/// Condition that determines when builddiag should exit with failure.
///
/// This controls the exit code behavior based on finding severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    /// Fail only on error-level findings.
    Error,
    /// Fail on warning or error-level findings.
    Warn,
    /// Never fail based on findings (always exit 0).
    Never,
}

/// Profile preset that configures check severities and behavior.
///
/// Profiles provide sensible defaults for different use cases:
/// - `Oss` - Warn-heavy defaults suitable for open source projects
/// - `Team` - Stronger gating for team/organization projects
/// - `Strict` - Full CI/release discipline with maximum enforcement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Open source profile with warn-heavy defaults and minimal assumptions.
    /// Good for wide adoption with low friction.
    #[default]
    Oss,
    /// Team profile with stronger gating for organizational projects.
    Team,
    /// Strict profile with maximum enforcement for CI/release discipline.
    Strict,
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Profile::Oss => write!(f, "oss"),
            Profile::Team => write!(f, "team"),
            Profile::Strict => write!(f, "strict"),
        }
    }
}

impl std::str::FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "oss" => Ok(Profile::Oss),
            "team" => Ok(Profile::Team),
            "strict" => Ok(Profile::Strict),
            _ => Err(format!(
                "invalid profile '{}': expected 'oss', 'team', or 'strict'",
                s
            )),
        }
    }
}

/// Check enablement state for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileCheckState {
    /// Check is enabled with the given severity.
    Enabled(Severity),
    /// Check is skipped/disabled.
    Skip,
}

/// Effective configuration for a single check after combining profile defaults with user overrides.
///
/// This struct represents the final resolved configuration for a check, computed by
/// [`effective_check_config`]. It combines:
/// 1. Profile defaults (severity and enabled state based on selected profile)
/// 2. User overrides from the config file's `[[checks]]` section
///
/// # Example
///
/// ```
/// use builddiag_types::{Config, Profile, Severity, effective_check_config};
///
/// let mut config = Config::default();
/// config.profile = Profile::Team;
///
/// let effective = effective_check_config(&config, "rust.msrv_defined");
/// assert!(effective.enabled);
/// assert_eq!(effective.severity, Severity::Warn);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveCheckConfig {
    /// Whether this check is enabled.
    pub enabled: bool,
    /// Severity for findings from this check.
    pub severity: Severity,
}

impl Default for EffectiveCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            severity: Severity::Error,
        }
    }
}

/// Computes the effective configuration for a check by combining profile defaults with user overrides.
///
/// This function provides a single point of configuration resolution, eliminating
/// scattered branching throughout the codebase. The resolution order is:
///
/// 1. Start with the profile's default state for the check
/// 2. Apply any user overrides from `config.checks` if present
///
/// # Arguments
///
/// * `config` - The loaded configuration (with profile and optional check overrides)
/// * `check_id` - The check identifier (e.g., "rust.msrv_defined")
///
/// # Returns
///
/// An [`EffectiveCheckConfig`] with the final enabled state and severity.
///
/// # Example
///
/// ```
/// use builddiag_types::{Config, Profile, Severity, CheckConfig, effective_check_config};
///
/// // Profile defaults
/// let mut config = Config::default();
/// config.profile = Profile::Oss;
///
/// let effective = effective_check_config(&config, "rust.msrv_defined");
/// assert!(effective.enabled);
/// assert_eq!(effective.severity, Severity::Warn);
///
/// // User override takes precedence
/// config.checks.push(CheckConfig {
///     id: "rust.msrv_defined".to_string(),
///     severity: Severity::Error,
///     enabled: true,
///     triggers: vec![],
/// });
///
/// let effective = effective_check_config(&config, "rust.msrv_defined");
/// assert_eq!(effective.severity, Severity::Error);
/// ```
pub fn effective_check_config(config: &Config, check_id: &str) -> EffectiveCheckConfig {
    // Start with profile defaults
    let profile_state = config.profile.check_state(check_id);

    let (profile_enabled, profile_severity) = match profile_state {
        ProfileCheckState::Enabled(sev) => (true, sev),
        ProfileCheckState::Skip => (false, Severity::Error), // Default severity if re-enabled
    };

    // Check for user override
    let user_override = config.checks.iter().find(|c| c.id == check_id);

    match user_override {
        Some(ov) => EffectiveCheckConfig {
            enabled: ov.enabled,
            severity: ov.severity,
        },
        None => EffectiveCheckConfig {
            enabled: profile_enabled,
            severity: profile_severity,
        },
    }
}

impl Profile {
    /// Returns the check state (severity or skip) for a given check ID under this profile.
    ///
    /// # Profile Severity Mappings
    ///
    /// ## `oss` (default) - permissive, never fails on conventions
    /// - rust.msrv_defined: enabled, warn
    /// - rust.msrv_consistent: enabled, error (real footgun)
    /// - rust.toolchain_pinning: enabled, info
    /// - rust.toolchain_msrv_relation: enabled, warn
    /// - workspace.resolver_v2: enabled, info
    /// - tools.*: disabled (skip)
    ///
    /// ## `team` - reasonable gating for disciplined repos
    /// - rust.msrv_defined: enabled, warn
    /// - rust.msrv_consistent: enabled, error
    /// - rust.toolchain_pinning: enabled, warn
    /// - rust.toolchain_msrv_relation: enabled, error
    /// - workspace.resolver_v2: enabled, warn
    /// - tools.*: enabled, warn
    ///
    /// ## `strict` - CI/release discipline
    /// - All checks: enabled, error
    pub fn check_state(&self, check_id: &str) -> ProfileCheckState {
        match self {
            Profile::Oss => match check_id {
                "rust.msrv_defined" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.msrv_consistent" => ProfileCheckState::Enabled(Severity::Error),
                "rust.toolchain_pinning" => ProfileCheckState::Enabled(Severity::Info),
                "rust.toolchain_msrv_relation" => ProfileCheckState::Enabled(Severity::Warn),
                "workspace.resolver_v2" => ProfileCheckState::Enabled(Severity::Info),
                "workspace.edition_consistent" => ProfileCheckState::Enabled(Severity::Warn),
                "workspace.member_ordering" => ProfileCheckState::Enabled(Severity::Info),
                // All tools.* checks are skipped in oss profile
                id if id.starts_with("tools.") => ProfileCheckState::Skip,
                // All deps.* checks are info in oss profile
                id if id.starts_with("deps.") => ProfileCheckState::Enabled(Severity::Info),
                // Unknown checks default to warn
                _ => ProfileCheckState::Enabled(Severity::Warn),
            },
            Profile::Team => match check_id {
                "rust.msrv_defined" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.msrv_consistent" => ProfileCheckState::Enabled(Severity::Error),
                "rust.toolchain_pinning" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.toolchain_msrv_relation" => ProfileCheckState::Enabled(Severity::Error),
                "workspace.resolver_v2" => ProfileCheckState::Enabled(Severity::Warn),
                "workspace.edition_consistent" => ProfileCheckState::Enabled(Severity::Error),
                "workspace.member_ordering" => ProfileCheckState::Enabled(Severity::Info),
                // All tools.* checks are warn in team profile
                id if id.starts_with("tools.") => ProfileCheckState::Enabled(Severity::Warn),
                // All deps.* checks are warn in team profile
                id if id.starts_with("deps.") => ProfileCheckState::Enabled(Severity::Warn),
                // Unknown checks default to warn
                _ => ProfileCheckState::Enabled(Severity::Warn),
            },
            // All checks at error severity in strict mode
            Profile::Strict => ProfileCheckState::Enabled(Severity::Error),
        }
    }
}

/// Source for determining the authoritative MSRV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MsrvSource {
    /// MSRV must be defined at the workspace level.
    Workspace,
    /// MSRV can be defined in any crate.
    Any,
}

/// Required relationship between toolchain version and MSRV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RelationToMsrv {
    /// Toolchain version must exactly equal MSRV.
    Equals,
    /// Toolchain version must be at least MSRV.
    AtLeast,
}

/// Default settings for builddiag behavior.
///
/// These settings control output location, failure conditions, and
/// diff-aware mode for incremental checking.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Defaults {
    /// Condition that triggers a non-zero exit code.
    #[serde(default = "Defaults::default_fail_on")]
    pub fail_on: FailOn,
    /// Directory for output files (report.json, comment.md).
    #[serde(default = "Defaults::default_out_dir")]
    pub out_dir: String,
    /// Enable diff-aware mode to only check changed files.
    #[serde(default)]
    pub diff_aware: bool,
    /// Base git ref for diff-aware mode.
    #[serde(default = "Defaults::default_base")]
    pub base: String,
    /// Head git ref for diff-aware mode.
    #[serde(default = "Defaults::default_head")]
    pub head: String,
}

impl Defaults {
    fn default_fail_on() -> FailOn {
        FailOn::Error
    }
    fn default_out_dir() -> String {
        "artifacts/builddiag".to_string()
    }
    fn default_base() -> String {
        "origin/main".to_string()
    }
    fn default_head() -> String {
        "HEAD".to_string()
    }
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            fail_on: FailOn::Error,
            out_dir: Self::default_out_dir(),
            diff_aware: false,
            base: Self::default_base(),
            head: Self::default_head(),
        }
    }
}

/// Configuration for input file paths.
///
/// All paths are relative to the repository root. These can be customized
/// if your project uses non-standard locations for configuration files.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PathsConfig {
    /// Path to the root Cargo.toml file.
    #[serde(default = "PathsConfig::default_cargo_root")]
    pub cargo_root: String,
    /// Path to the rust-toolchain.toml file.
    #[serde(default = "PathsConfig::default_rust_toolchain")]
    pub rust_toolchain: String,
    /// Path to the tools checksums file.
    #[serde(default = "PathsConfig::default_tools_checksums")]
    pub tools_checksums: String,
    /// Path to the tools manifest file.
    #[serde(default = "PathsConfig::default_tools_manifest")]
    pub tools_manifest: String,
}

impl PathsConfig {
    fn default_cargo_root() -> String {
        "Cargo.toml".to_string()
    }
    fn default_rust_toolchain() -> String {
        "rust-toolchain.toml".to_string()
    }
    fn default_tools_checksums() -> String {
        "scripts/tools.sha256".to_string()
    }
    fn default_tools_manifest() -> String {
        "scripts/tools.toml".to_string()
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            cargo_root: Self::default_cargo_root(),
            rust_toolchain: Self::default_rust_toolchain(),
            tools_checksums: Self::default_tools_checksums(),
            tools_manifest: Self::default_tools_manifest(),
        }
    }
}

/// Policy settings for MSRV (Minimum Supported Rust Version) checks.
///
/// Controls how builddiag validates MSRV configuration across the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MsrvPolicy {
    /// Require MSRV to be explicitly defined.
    #[serde(default = "MsrvPolicy::default_require_defined")]
    pub require_defined: bool,
    /// Where MSRV should be defined (workspace or any crate).
    #[serde(default = "MsrvPolicy::default_source")]
    pub source: MsrvSource,
    /// Allow individual crates to override the workspace MSRV.
    #[serde(default)]
    pub allow_per_crate_override: bool,
    /// List of crate names allowed to have different MSRV.
    #[serde(default)]
    pub allow_overrides: Vec<String>,
}

impl MsrvPolicy {
    fn default_require_defined() -> bool {
        true
    }
    fn default_source() -> MsrvSource {
        MsrvSource::Workspace
    }
}

impl Default for MsrvPolicy {
    fn default() -> Self {
        Self {
            require_defined: true,
            source: MsrvSource::Workspace,
            allow_per_crate_override: false,
            allow_overrides: Vec::new(),
        }
    }
}

/// Policy settings for toolchain pinning checks.
///
/// Controls how builddiag validates the rust-toolchain.toml configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolchainPolicy {
    /// Require toolchain to be pinned to a specific version.
    #[serde(default = "ToolchainPolicy::default_require_pinned")]
    pub require_pinned: bool,
    /// Required relationship between toolchain version and MSRV.
    #[serde(default = "ToolchainPolicy::default_relation")]
    pub relation_to_msrv: RelationToMsrv,
    /// Allow nightly toolchain to be pinned.
    #[serde(default)]
    pub allow_nightly: bool,
}

impl ToolchainPolicy {
    fn default_require_pinned() -> bool {
        true
    }
    fn default_relation() -> RelationToMsrv {
        RelationToMsrv::Equals
    }
}

impl Default for ToolchainPolicy {
    fn default() -> Self {
        Self {
            require_pinned: true,
            relation_to_msrv: RelationToMsrv::Equals,
            allow_nightly: false,
        }
    }
}

/// Policy settings for tool checksums verification.
///
/// Controls how builddiag validates tool checksums files.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChecksumsPolicy {
    /// Require a checksums file to exist.
    #[serde(default = "ChecksumsPolicy::default_require_file")]
    pub require_file: bool,
    /// Require all tools in manifest to have checksums.
    #[serde(default)]
    pub require_coverage: bool,
    /// Verify checksums against local files.
    #[serde(default)]
    pub verify_local_files: bool,
}

impl ChecksumsPolicy {
    fn default_require_file() -> bool {
        true
    }
}

impl Default for ChecksumsPolicy {
    fn default() -> Self {
        Self {
            require_file: true,
            require_coverage: false,
            verify_local_files: false,
        }
    }
}

/// Policy settings for edition consistency checks.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditionPolicy {
    /// Require all crates to have consistent edition.
    #[serde(default = "EditionPolicy::default_require_consistent")]
    pub require_consistent: bool,
    /// Allow individual crates to override the workspace edition.
    #[serde(default)]
    pub allow_per_crate_override: bool,
    /// List of crate paths allowed to have different edition.
    #[serde(default)]
    pub allow_overrides: Vec<String>,
}

impl EditionPolicy {
    fn default_require_consistent() -> bool {
        true
    }
}

impl Default for EditionPolicy {
    fn default() -> Self {
        Self {
            require_consistent: true,
            allow_per_crate_override: false,
            allow_overrides: Vec::new(),
        }
    }
}

/// Policy settings for member ordering checks.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemberOrderingPolicy {
    /// Require workspace.members to be sorted alphabetically.
    #[serde(default = "MemberOrderingPolicy::default_require_sorted")]
    pub require_sorted: bool,
}

impl MemberOrderingPolicy {
    fn default_require_sorted() -> bool {
        true
    }
}

impl Default for MemberOrderingPolicy {
    fn default() -> Self {
        Self {
            require_sorted: true,
        }
    }
}

/// Policy settings for lockfile checks.
///
/// Controls how builddiag validates Cargo.lock presence.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LockfilePolicy {
    /// Require Cargo.lock for binary crates.
    #[serde(default = "LockfilePolicy::default_require_for_binaries")]
    pub require_for_binaries: bool,
    /// Warn about lockfile in library-only crates.
    #[serde(default)]
    pub warn_for_libraries: bool,
}

impl LockfilePolicy {
    fn default_require_for_binaries() -> bool {
        true
    }
}

impl Default for LockfilePolicy {
    fn default() -> Self {
        Self {
            require_for_binaries: true,
            warn_for_libraries: false,
        }
    }
}

/// Combined policy settings for all check categories.
///
/// Groups together MSRV, toolchain, checksums, edition, member ordering, and lockfile policies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Policy {
    /// MSRV validation policy.
    #[serde(default)]
    pub msrv: MsrvPolicy,
    /// Toolchain pinning policy.
    #[serde(default)]
    pub toolchain: ToolchainPolicy,
    /// Checksums verification policy.
    #[serde(default)]
    pub checksums: ChecksumsPolicy,
    /// Edition consistency policy.
    #[serde(default)]
    pub edition: EditionPolicy,
    /// Member ordering policy.
    #[serde(default)]
    pub member_ordering: MemberOrderingPolicy,
    /// Lockfile policy.
    #[serde(default)]
    pub lockfile: LockfilePolicy,
}

/// Per-check configuration override.
///
/// Allows customizing individual check behavior, including severity
/// and whether the check is enabled.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckConfig {
    /// Unique identifier for the check to configure.
    pub id: String,
    /// Override severity for findings from this check.
    #[serde(default = "CheckConfig::default_severity")]
    pub severity: Severity,
    /// Whether this check is enabled.
    #[serde(default = "CheckConfig::default_enabled")]
    pub enabled: bool,
    /// File patterns that trigger this check in diff-aware mode.
    #[serde(default)]
    pub triggers: Vec<String>,
}

impl CheckConfig {
    fn default_severity() -> Severity {
        Severity::Error
    }
    fn default_enabled() -> bool {
        true
    }
}

/// Main configuration for builddiag.
///
/// This is typically loaded from a `.builddiag.toml` file in the repository
/// root. All fields have sensible defaults, so an empty config is valid.
///
/// # Example
///
/// ```toml
/// profile = "oss"
///
/// [defaults]
/// fail_on = "error"
/// out_dir = "artifacts/builddiag"
///
/// [policy.msrv]
/// require_defined = true
/// source = "workspace"
///
/// [policy.toolchain]
/// require_pinned = true
/// relation_to_msrv = "equals"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    /// Profile preset that configures check severities.
    /// Can be overridden via CLI `--profile` flag.
    #[serde(default)]
    pub profile: Profile,
    /// Default settings for output and failure behavior.
    #[serde(default)]
    pub defaults: Defaults,
    /// Paths to input files.
    #[serde(default)]
    pub paths: PathsConfig,
    /// Policy settings for checks.
    #[serde(default)]
    pub policy: Policy,
    /// Per-check configuration overrides.
    #[serde(default)]
    pub checks: Vec<CheckConfig>,
    /// Optional arbitrary metadata for downstream tooling.
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

impl Config {
    /// Returns a map of check IDs to their configuration overrides.
    ///
    /// This is useful for looking up per-check settings by ID.
    pub fn check_overrides(&self) -> BTreeMap<String, CheckConfig> {
        let mut map = BTreeMap::new();
        for c in &self.checks {
            map.insert(c.id.clone(), c.clone());
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests for Config default values
    /// _Requirements: 2.1_
    mod config_defaults {
        use super::*;

        #[test]
        fn config_default_has_expected_defaults_field() {
            let config = Config::default();
            let defaults = config.defaults;

            assert_eq!(defaults.fail_on, FailOn::Error);
            assert_eq!(defaults.out_dir, "artifacts/builddiag");
            assert!(!defaults.diff_aware);
            assert_eq!(defaults.base, "origin/main");
            assert_eq!(defaults.head, "HEAD");
        }

        #[test]
        fn config_default_has_expected_paths_field() {
            let config = Config::default();
            let paths = config.paths;

            assert_eq!(paths.cargo_root, "Cargo.toml");
            assert_eq!(paths.rust_toolchain, "rust-toolchain.toml");
            assert_eq!(paths.tools_checksums, "scripts/tools.sha256");
            assert_eq!(paths.tools_manifest, "scripts/tools.toml");
        }

        #[test]
        fn config_default_has_expected_policy_field() {
            let config = Config::default();
            let policy = config.policy;

            // MSRV policy defaults
            assert!(policy.msrv.require_defined);
            assert_eq!(policy.msrv.source, MsrvSource::Workspace);
            assert!(!policy.msrv.allow_per_crate_override);
            assert!(policy.msrv.allow_overrides.is_empty());

            // Toolchain policy defaults
            assert!(policy.toolchain.require_pinned);
            assert_eq!(policy.toolchain.relation_to_msrv, RelationToMsrv::Equals);
            assert!(!policy.toolchain.allow_nightly);

            // Checksums policy defaults
            assert!(policy.checksums.require_file);
            assert!(!policy.checksums.require_coverage);
            assert!(!policy.checksums.verify_local_files);
        }

        #[test]
        fn config_default_has_empty_checks_and_meta() {
            let config = Config::default();

            assert!(config.checks.is_empty());
            assert!(config.meta.is_empty());
        }
    }

    /// Tests for nested type defaults
    /// _Requirements: 2.1_
    mod nested_type_defaults {
        use super::*;

        #[test]
        fn defaults_struct_has_expected_values() {
            let defaults = Defaults::default();

            assert_eq!(defaults.fail_on, FailOn::Error);
            assert_eq!(defaults.out_dir, "artifacts/builddiag");
            assert!(!defaults.diff_aware);
            assert_eq!(defaults.base, "origin/main");
            assert_eq!(defaults.head, "HEAD");
        }

        #[test]
        fn paths_config_has_expected_values() {
            let paths = PathsConfig::default();

            assert_eq!(paths.cargo_root, "Cargo.toml");
            assert_eq!(paths.rust_toolchain, "rust-toolchain.toml");
            assert_eq!(paths.tools_checksums, "scripts/tools.sha256");
            assert_eq!(paths.tools_manifest, "scripts/tools.toml");
        }

        #[test]
        fn policy_has_expected_default_values() {
            let policy = Policy::default();

            // Verify nested defaults are applied
            assert!(policy.msrv.require_defined);
            assert!(policy.toolchain.require_pinned);
            assert!(policy.checksums.require_file);
        }

        #[test]
        fn msrv_policy_has_expected_values() {
            let msrv = MsrvPolicy::default();

            assert!(msrv.require_defined);
            assert_eq!(msrv.source, MsrvSource::Workspace);
            assert!(!msrv.allow_per_crate_override);
            assert!(msrv.allow_overrides.is_empty());
        }

        #[test]
        fn toolchain_policy_has_expected_values() {
            let toolchain = ToolchainPolicy::default();

            assert!(toolchain.require_pinned);
            assert_eq!(toolchain.relation_to_msrv, RelationToMsrv::Equals);
            assert!(!toolchain.allow_nightly);
        }

        #[test]
        fn checksums_policy_has_expected_values() {
            let checksums = ChecksumsPolicy::default();

            assert!(checksums.require_file);
            assert!(!checksums.require_coverage);
            assert!(!checksums.verify_local_files);
        }
    }

    /// Tests for check_overrides() method
    /// _Requirements: 2.1_
    mod check_overrides_tests {
        use super::*;

        #[test]
        fn check_overrides_returns_empty_map_for_default_config() {
            let config = Config::default();
            let overrides = config.check_overrides();

            assert!(overrides.is_empty());
        }

        #[test]
        fn check_overrides_returns_map_with_single_check() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "msrv_defined".to_string(),
                severity: Severity::Warn,
                enabled: true,
                triggers: vec!["Cargo.toml".to_string()],
            });

            let overrides = config.check_overrides();

            assert_eq!(overrides.len(), 1);
            assert!(overrides.contains_key("msrv_defined"));

            let check = overrides.get("msrv_defined").unwrap();
            assert_eq!(check.id, "msrv_defined");
            assert_eq!(check.severity, Severity::Warn);
            assert!(check.enabled);
            assert_eq!(check.triggers, vec!["Cargo.toml".to_string()]);
        }

        #[test]
        fn check_overrides_returns_map_with_multiple_checks() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "msrv_defined".to_string(),
                severity: Severity::Error,
                enabled: true,
                triggers: vec![],
            });
            config.checks.push(CheckConfig {
                id: "toolchain_pinned".to_string(),
                severity: Severity::Warn,
                enabled: false,
                triggers: vec!["rust-toolchain.toml".to_string()],
            });
            config.checks.push(CheckConfig {
                id: "checksums_valid".to_string(),
                severity: Severity::Info,
                enabled: true,
                triggers: vec![],
            });

            let overrides = config.check_overrides();

            assert_eq!(overrides.len(), 3);
            assert!(overrides.contains_key("msrv_defined"));
            assert!(overrides.contains_key("toolchain_pinned"));
            assert!(overrides.contains_key("checksums_valid"));

            // Verify ordering is deterministic (BTreeMap)
            let keys: Vec<_> = overrides.keys().collect();
            assert_eq!(
                keys,
                vec!["checksums_valid", "msrv_defined", "toolchain_pinned"]
            );
        }

        #[test]
        fn check_overrides_last_duplicate_wins() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "msrv_defined".to_string(),
                severity: Severity::Error,
                enabled: true,
                triggers: vec![],
            });
            config.checks.push(CheckConfig {
                id: "msrv_defined".to_string(),
                severity: Severity::Warn,
                enabled: false,
                triggers: vec!["override".to_string()],
            });

            let overrides = config.check_overrides();

            assert_eq!(overrides.len(), 1);
            let check = overrides.get("msrv_defined").unwrap();
            // Last entry should win
            assert_eq!(check.severity, Severity::Warn);
            assert!(!check.enabled);
            assert_eq!(check.triggers, vec!["override".to_string()]);
        }
    }

    /// Tests for CheckConfig defaults
    /// _Requirements: 2.1_
    mod check_config_defaults {
        use super::*;

        #[test]
        fn check_config_default_severity_is_error() {
            assert_eq!(CheckConfig::default_severity(), Severity::Error);
        }

        #[test]
        fn check_config_default_enabled_is_true() {
            assert!(CheckConfig::default_enabled());
        }
    }

    /// Tests for Finding construction with various severity levels
    /// _Requirements: 2.1_
    mod finding_tests {
        use super::*;

        #[test]
        fn finding_with_error_severity() {
            let finding = Finding {
                check_id: "rust.msrv_defined".to_string(),
                code: "missing_msrv".to_string(),
                severity: Severity::Error,
                message: "Missing rust-version in Cargo.toml".to_string(),
                location: Some(Location {
                    path: "Cargo.toml".to_string(),
                    line: Some(1),
                    col: Some(5),
                }),
            };

            assert_eq!(finding.check_id, "rust.msrv_defined");
            assert_eq!(finding.severity, Severity::Error);
            assert_eq!(finding.code, "missing_msrv");
            assert_eq!(finding.message, "Missing rust-version in Cargo.toml");
            assert!(finding.location.is_some());
            let loc = finding.location.unwrap();
            assert_eq!(loc.path, "Cargo.toml");
            assert_eq!(loc.line, Some(1));
            assert_eq!(loc.col, Some(5));
        }

        #[test]
        fn finding_with_warn_severity() {
            let finding = Finding {
                check_id: "rust.toolchain_msrv_relation".to_string(),
                code: "msrv_mismatch".to_string(),
                severity: Severity::Warn,
                message: "MSRV differs from toolchain version".to_string(),
                location: Some(Location {
                    path: "rust-toolchain.toml".to_string(),
                    line: None,
                    col: None,
                }),
            };

            assert_eq!(finding.severity, Severity::Warn);
            assert_eq!(finding.code, "msrv_mismatch");
            assert!(finding.location.is_some());
            let loc = finding.location.unwrap();
            assert_eq!(loc.path, "rust-toolchain.toml");
            assert!(loc.line.is_none());
            assert!(loc.col.is_none());
        }

        #[test]
        fn finding_with_info_severity() {
            let finding = Finding {
                check_id: "workspace.info".to_string(),
                code: "workspace_detected".to_string(),
                severity: Severity::Info,
                message: "Workspace with 5 members detected".to_string(),
                location: None,
            };

            assert_eq!(finding.severity, Severity::Info);
            assert_eq!(finding.code, "workspace_detected");
            assert!(finding.location.is_none());
        }

        #[test]
        fn finding_without_location() {
            let finding = Finding {
                check_id: "general".to_string(),
                code: "general_error".to_string(),
                severity: Severity::Error,
                message: "A general error occurred".to_string(),
                location: None,
            };

            assert!(finding.location.is_none());
        }

        #[test]
        fn severity_ordering() {
            // Verify severity ordering: Info < Warn < Error
            assert!(Severity::Info < Severity::Warn);
            assert!(Severity::Warn < Severity::Error);
            assert!(Severity::Info < Severity::Error);
        }

        #[test]
        fn finding_equality() {
            let finding1 = Finding {
                check_id: "test".to_string(),
                code: "test".to_string(),
                severity: Severity::Error,
                message: "Test message".to_string(),
                location: Some(Location {
                    path: "test.rs".to_string(),
                    line: Some(10),
                    col: Some(5),
                }),
            };

            let finding2 = Finding {
                check_id: "test".to_string(),
                code: "test".to_string(),
                severity: Severity::Error,
                message: "Test message".to_string(),
                location: Some(Location {
                    path: "test.rs".to_string(),
                    line: Some(10),
                    col: Some(5),
                }),
            };

            assert_eq!(finding1, finding2);
        }

        #[test]
        fn finding_clone() {
            let finding = Finding {
                check_id: "clone_test".to_string(),
                code: "clone_test".to_string(),
                severity: Severity::Warn,
                message: "Testing clone".to_string(),
                location: Some(Location {
                    path: "file.rs".to_string(),
                    line: Some(42),
                    col: None,
                }),
            };

            let cloned = finding.clone();
            assert_eq!(finding, cloned);
        }

        #[test]
        fn location_construction() {
            let loc = Location {
                path: "src/lib.rs".to_string(),
                line: Some(42),
                col: Some(10),
            };
            assert_eq!(loc.path, "src/lib.rs");
            assert_eq!(loc.line, Some(42));
            assert_eq!(loc.col, Some(10));
        }

        #[test]
        fn location_without_line_col() {
            let loc = Location {
                path: "Cargo.toml".to_string(),
                line: None,
                col: None,
            };
            assert_eq!(loc.path, "Cargo.toml");
            assert!(loc.line.is_none());
            assert!(loc.col.is_none());
        }
    }

    /// Tests for CheckReport construction with different statuses
    /// _Requirements: 2.1_
    mod check_report_tests {
        use super::*;

        fn make_finding(check_id: &str, severity: Severity, code: &str) -> Finding {
            Finding {
                check_id: check_id.to_string(),
                code: code.to_string(),
                severity,
                message: format!("Test finding: {}", code),
                location: None,
            }
        }

        #[test]
        fn check_report_with_pass_status() {
            let report = CheckReport {
                id: "msrv_defined".to_string(),
                status: CheckStatus::Pass,
                findings: vec![],
                skipped_reason: None,
            };

            assert_eq!(report.id, "msrv_defined");
            assert_eq!(report.status, CheckStatus::Pass);
            assert!(report.findings.is_empty());
            assert!(report.skipped_reason.is_none());
        }

        #[test]
        fn check_report_with_warn_status() {
            let finding = make_finding("toolchain_check", Severity::Warn, "msrv_mismatch");

            let report = CheckReport {
                id: "toolchain_check".to_string(),
                status: CheckStatus::Warn,
                findings: vec![finding],
                skipped_reason: None,
            };

            assert_eq!(report.status, CheckStatus::Warn);
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.findings[0].severity, Severity::Warn);
        }

        #[test]
        fn check_report_with_fail_status() {
            let finding = make_finding("msrv_defined", Severity::Error, "missing_msrv");

            let report = CheckReport {
                id: "msrv_defined".to_string(),
                status: CheckStatus::Fail,
                findings: vec![finding],
                skipped_reason: None,
            };

            assert_eq!(report.status, CheckStatus::Fail);
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.findings[0].severity, Severity::Error);
        }

        #[test]
        fn check_report_with_skip_status() {
            let report = CheckReport {
                id: "checksums_valid".to_string(),
                status: CheckStatus::Skip,
                findings: vec![],
                skipped_reason: Some("Checksums file not found".to_string()),
            };

            assert_eq!(report.status, CheckStatus::Skip);
            assert!(report.findings.is_empty());
            assert_eq!(
                report.skipped_reason,
                Some("Checksums file not found".to_string())
            );
        }

        #[test]
        fn check_report_with_multiple_findings() {
            let findings = vec![
                make_finding("multi_finding_check", Severity::Error, "error1"),
                make_finding("multi_finding_check", Severity::Warn, "warn1"),
                make_finding("multi_finding_check", Severity::Info, "info1"),
            ];

            let report = CheckReport {
                id: "multi_finding_check".to_string(),
                status: CheckStatus::Fail,
                findings,
                skipped_reason: None,
            };

            assert_eq!(report.findings.len(), 3);
            assert_eq!(report.findings[0].severity, Severity::Error);
            assert_eq!(report.findings[1].severity, Severity::Warn);
            assert_eq!(report.findings[2].severity, Severity::Info);
        }

        #[test]
        fn check_report_equality() {
            let report1 = CheckReport {
                id: "test_check".to_string(),
                status: CheckStatus::Pass,
                findings: vec![],
                skipped_reason: None,
            };

            let report2 = CheckReport {
                id: "test_check".to_string(),
                status: CheckStatus::Pass,
                findings: vec![],
                skipped_reason: None,
            };

            assert_eq!(report1, report2);
        }

        #[test]
        fn check_status_variants() {
            // Verify all CheckStatus variants can be constructed
            assert_eq!(CheckStatus::Pass, CheckStatus::Pass);
            assert_eq!(CheckStatus::Warn, CheckStatus::Warn);
            assert_eq!(CheckStatus::Fail, CheckStatus::Fail);
            assert_eq!(CheckStatus::Skip, CheckStatus::Skip);

            // Verify they are distinct
            assert_ne!(CheckStatus::Pass, CheckStatus::Fail);
            assert_ne!(CheckStatus::Warn, CheckStatus::Skip);
        }
    }

    /// Tests for Summary construction with counts
    /// _Requirements: 2.1_
    mod summary_tests {
        use super::*;

        #[test]
        fn summary_construction() {
            let mut by_severity = BTreeMap::new();
            by_severity.insert("error".to_string(), 2);
            by_severity.insert("warn".to_string(), 3);
            by_severity.insert("info".to_string(), 1);

            let mut by_check = BTreeMap::new();
            by_check.insert("rust.msrv_defined".to_string(), 1);
            by_check.insert("rust.msrv_consistent".to_string(), 2);

            let summary = Summary {
                total_findings: 6,
                by_severity,
                by_check,
            };

            assert_eq!(summary.total_findings, 6);
            assert_eq!(summary.by_severity.get("error"), Some(&2));
            assert_eq!(summary.by_severity.get("warn"), Some(&3));
            assert_eq!(summary.by_severity.get("info"), Some(&1));
            assert_eq!(summary.by_check.len(), 2);
        }

        #[test]
        fn summary_empty() {
            let summary = Summary {
                total_findings: 0,
                by_severity: BTreeMap::new(),
                by_check: BTreeMap::new(),
            };

            assert_eq!(summary.total_findings, 0);
            assert!(summary.by_severity.is_empty());
            assert!(summary.by_check.is_empty());
        }

        #[test]
        fn verdict_variants() {
            // Verify all Verdict variants can be constructed
            assert_eq!(Verdict::Pass, Verdict::Pass);
            assert_eq!(Verdict::Warn, Verdict::Warn);
            assert_eq!(Verdict::Fail, Verdict::Fail);
            assert_eq!(Verdict::Skip, Verdict::Skip);
            assert_eq!(Verdict::Error, Verdict::Error);

            // Verify they are distinct
            assert_ne!(Verdict::Pass, Verdict::Fail);
            assert_ne!(Verdict::Warn, Verdict::Skip);
            assert_ne!(Verdict::Fail, Verdict::Error);
        }

        #[test]
        fn summary_counts_legacy() {
            let counts = SummaryCounts {
                info: 5,
                warn: 10,
                error: 15,
            };

            assert_eq!(counts.info, 5);
            assert_eq!(counts.warn, 10);
            assert_eq!(counts.error, 15);
        }

        #[test]
        fn summary_equality() {
            let mut by_severity = BTreeMap::new();
            by_severity.insert("error".to_string(), 1);

            let summary1 = Summary {
                total_findings: 1,
                by_severity: by_severity.clone(),
                by_check: BTreeMap::new(),
            };

            let summary2 = Summary {
                total_findings: 1,
                by_severity,
                by_check: BTreeMap::new(),
            };

            assert_eq!(summary1, summary2);
        }
    }

    /// Tests for Report construction with all fields
    /// _Requirements: 2.1_
    mod report_tests {
        use super::*;
        use chrono::TimeZone;

        fn create_test_report() -> Report {
            let started = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
            let ended = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 5).unwrap();

            let mut by_severity = BTreeMap::new();
            by_severity.insert("warn".to_string(), 1);

            let mut by_check = BTreeMap::new();
            by_check.insert("rust.toolchain_msrv_relation".to_string(), 1);

            Report {
                schema: Report::SCHEMA_V1.to_string(),
                tool: Some(ToolInfo {
                    name: "builddiag".to_string(),
                    version: "0.1.0".to_string(),
                }),
                run: Some(RunInfo {
                    started_at: started,
                    ended_at: Some(ended),
                    duration_ms: 5000,
                    host: HostInfo {
                        os: "linux".to_string(),
                        arch: "x86_64".to_string(),
                    },
                    git: Some(GitInfo {
                        commit: "abc123".to_string(),
                        branch: Some("main".to_string()),
                        dirty: false,
                    }),
                }),
                verdict: Verdict::Warn,
                findings: vec![Finding {
                    check_id: "rust.toolchain_msrv_relation".to_string(),
                    code: "toolchain_mismatch".to_string(),
                    severity: Severity::Warn,
                    message: "Toolchain version differs from MSRV".to_string(),
                    location: Some(Location {
                        path: "rust-toolchain.toml".to_string(),
                        line: None,
                        col: None,
                    }),
                }],
                summary: Some(Summary {
                    total_findings: 1,
                    by_severity,
                    by_check,
                }),
                data: None,
            }
        }

        #[test]
        fn report_construction_with_all_fields() {
            let report = create_test_report();

            assert_eq!(report.schema, Report::SCHEMA_V1);
            let tool = report.tool.as_ref().expect("tool info should be present");
            assert_eq!(tool.name, "builddiag");
            assert_eq!(tool.version, "0.1.0");
            let run = report.run.as_ref().expect("run info should be present");
            assert_eq!(run.duration_ms, 5000);
            assert!(run.ended_at.is_some());
            assert_eq!(run.host.os, "linux");
            assert!(run.git.is_some());
            assert_eq!(report.verdict, Verdict::Warn);
            assert_eq!(report.findings.len(), 1);
            assert!(report.summary.is_some());
            assert!(report.data.is_none());
        }

        #[test]
        fn report_schema_constant() {
            assert_eq!(Report::SCHEMA_V1, "builddiag.report.v1");
        }

        #[test]
        fn report_tool_info_construction() {
            let tool = ToolInfo {
                name: "builddiag".to_string(),
                version: "1.2.3".to_string(),
            };

            assert_eq!(tool.name, "builddiag");
            assert_eq!(tool.version, "1.2.3");
        }

        #[test]
        fn report_run_info_construction() {
            let started = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
            let ended = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 10).unwrap();

            let run = RunInfo {
                started_at: started,
                ended_at: Some(ended),
                duration_ms: 10000,
                host: HostInfo {
                    os: "macos".to_string(),
                    arch: "aarch64".to_string(),
                },
                git: None,
            };

            assert_eq!(run.started_at, started);
            assert_eq!(run.ended_at, Some(ended));
            assert_eq!(run.duration_ms, 10000);
            assert_eq!(run.host.os, "macos");
            assert!(run.git.is_none());
        }

        #[test]
        fn report_run_info_without_end_time() {
            let started = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

            let run = RunInfo {
                started_at: started,
                ended_at: None,
                duration_ms: 1000,
                host: HostInfo {
                    os: "windows".to_string(),
                    arch: "x86_64".to_string(),
                },
                git: None,
            };

            assert!(run.ended_at.is_none());
        }

        #[test]
        fn host_info_construction() {
            let host = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            };
            assert_eq!(host.os, "linux");
            assert_eq!(host.arch, "x86_64");
        }

        #[test]
        fn git_info_construction() {
            let git = GitInfo {
                commit: "abc123def456".to_string(),
                branch: Some("main".to_string()),
                dirty: false,
            };
            assert_eq!(git.commit, "abc123def456");
            assert_eq!(git.branch, Some("main".to_string()));
            assert!(!git.dirty);
        }

        #[test]
        fn git_info_detached_head() {
            let git = GitInfo {
                commit: "abc123".to_string(),
                branch: None,
                dirty: true,
            };
            assert!(git.branch.is_none());
            assert!(git.dirty);
        }

        #[test]
        fn report_with_empty_findings() {
            let started = Utc::now();

            let report = Report {
                schema: Report::SCHEMA_V1.to_string(),
                tool: Some(ToolInfo {
                    name: "builddiag".to_string(),
                    version: "0.1.0".to_string(),
                }),
                run: Some(RunInfo {
                    started_at: started,
                    ended_at: Some(Utc::now()),
                    duration_ms: 100,
                    host: HostInfo {
                        os: "linux".to_string(),
                        arch: "x86_64".to_string(),
                    },
                    git: None,
                }),
                verdict: Verdict::Pass,
                findings: vec![],
                summary: None,
                data: None,
            };

            assert!(report.findings.is_empty());
            assert_eq!(report.verdict, Verdict::Pass);
            assert!(report.summary.is_none());
        }

        #[test]
        fn report_clone() {
            let report = create_test_report();
            let cloned = report.clone();

            assert_eq!(report, cloned);
        }

        #[test]
        fn report_repo_info_construction() {
            let repo = RepoInfo {
                root: "/path/to/repo".to_string(),
                detected: RepoDetected {
                    is_workspace: false,
                    members: 1,
                },
            };

            assert_eq!(repo.root, "/path/to/repo");
            assert!(!repo.detected.is_workspace);
            assert_eq!(repo.detected.members, 1);
        }

        #[test]
        fn report_inputs_construction() {
            let inputs = Inputs {
                cargo_root: Some("Cargo.toml".to_string()),
                rust_toolchain: Some("rust-toolchain.toml".to_string()),
                tools_checksums: None,
                tools_manifest: None,
            };

            assert!(inputs.cargo_root.is_some());
            assert!(inputs.rust_toolchain.is_some());
            assert!(inputs.tools_checksums.is_none());
            assert!(inputs.tools_manifest.is_none());
        }
    }

    /// Tests for Profile enum and check state mapping
    /// _Requirements: Profile severity mappings_
    mod profile_tests {
        use super::*;

        #[test]
        fn profile_default_is_oss() {
            let profile = Profile::default();
            assert_eq!(profile, Profile::Oss);
        }

        #[test]
        fn profile_display() {
            assert_eq!(Profile::Oss.to_string(), "oss");
            assert_eq!(Profile::Team.to_string(), "team");
            assert_eq!(Profile::Strict.to_string(), "strict");
        }

        #[test]
        fn profile_from_str() {
            assert_eq!("oss".parse::<Profile>().unwrap(), Profile::Oss);
            assert_eq!("OSS".parse::<Profile>().unwrap(), Profile::Oss);
            assert_eq!("team".parse::<Profile>().unwrap(), Profile::Team);
            assert_eq!("Team".parse::<Profile>().unwrap(), Profile::Team);
            assert_eq!("strict".parse::<Profile>().unwrap(), Profile::Strict);
            assert_eq!("STRICT".parse::<Profile>().unwrap(), Profile::Strict);
            assert!("invalid".parse::<Profile>().is_err());
        }

        // OSS profile severity tests
        #[test]
        fn oss_profile_msrv_defined_is_warn() {
            let state = Profile::Oss.check_state("rust.msrv_defined");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        #[test]
        fn oss_profile_msrv_consistent_is_error() {
            let state = Profile::Oss.check_state("rust.msrv_consistent");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Error));
        }

        #[test]
        fn oss_profile_toolchain_pinning_is_info() {
            let state = Profile::Oss.check_state("rust.toolchain_pinning");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Info));
        }

        #[test]
        fn oss_profile_toolchain_msrv_relation_is_warn() {
            let state = Profile::Oss.check_state("rust.toolchain_msrv_relation");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        #[test]
        fn oss_profile_resolver_v2_is_info() {
            let state = Profile::Oss.check_state("workspace.resolver_v2");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Info));
        }

        #[test]
        fn oss_profile_tools_checks_are_skipped() {
            assert_eq!(
                Profile::Oss.check_state("tools.checksums_file_exists"),
                ProfileCheckState::Skip
            );
            assert_eq!(
                Profile::Oss.check_state("tools.checksums_format"),
                ProfileCheckState::Skip
            );
            assert_eq!(
                Profile::Oss.check_state("tools.checksums_coverage"),
                ProfileCheckState::Skip
            );
            assert_eq!(
                Profile::Oss.check_state("tools.checksums_verify_local"),
                ProfileCheckState::Skip
            );
        }

        #[test]
        fn oss_profile_unknown_check_defaults_to_warn() {
            let state = Profile::Oss.check_state("unknown.check");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        // Team profile severity tests
        #[test]
        fn team_profile_msrv_defined_is_warn() {
            let state = Profile::Team.check_state("rust.msrv_defined");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        #[test]
        fn team_profile_msrv_consistent_is_error() {
            let state = Profile::Team.check_state("rust.msrv_consistent");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Error));
        }

        #[test]
        fn team_profile_toolchain_pinning_is_warn() {
            let state = Profile::Team.check_state("rust.toolchain_pinning");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        #[test]
        fn team_profile_toolchain_msrv_relation_is_error() {
            let state = Profile::Team.check_state("rust.toolchain_msrv_relation");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Error));
        }

        #[test]
        fn team_profile_resolver_v2_is_warn() {
            let state = Profile::Team.check_state("workspace.resolver_v2");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Warn));
        }

        #[test]
        fn team_profile_tools_checks_are_warn() {
            assert_eq!(
                Profile::Team.check_state("tools.checksums_file_exists"),
                ProfileCheckState::Enabled(Severity::Warn)
            );
            assert_eq!(
                Profile::Team.check_state("tools.checksums_format"),
                ProfileCheckState::Enabled(Severity::Warn)
            );
            assert_eq!(
                Profile::Team.check_state("tools.checksums_coverage"),
                ProfileCheckState::Enabled(Severity::Warn)
            );
            assert_eq!(
                Profile::Team.check_state("tools.checksums_verify_local"),
                ProfileCheckState::Enabled(Severity::Warn)
            );
        }

        // Strict profile severity tests
        #[test]
        fn strict_profile_all_checks_are_error() {
            let checks = [
                "rust.msrv_defined",
                "rust.msrv_consistent",
                "rust.toolchain_pinning",
                "rust.toolchain_msrv_relation",
                "workspace.resolver_v2",
                "tools.checksums_file_exists",
                "tools.checksums_format",
                "tools.checksums_coverage",
                "tools.checksums_verify_local",
            ];

            for check_id in &checks {
                let state = Profile::Strict.check_state(check_id);
                assert_eq!(
                    state,
                    ProfileCheckState::Enabled(Severity::Error),
                    "Strict profile should have {} at error severity",
                    check_id
                );
            }
        }

        #[test]
        fn strict_profile_unknown_check_is_error() {
            let state = Profile::Strict.check_state("unknown.check");
            assert_eq!(state, ProfileCheckState::Enabled(Severity::Error));
        }
    }

    /// Tests for EffectiveCheckConfig and effective_check_config function
    /// _Requirements: Profile + override resolution_
    mod effective_check_config_tests {
        use super::*;

        #[test]
        fn effective_config_uses_profile_defaults() {
            let config = Config::default(); // Uses Oss profile

            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Warn);
        }

        #[test]
        fn effective_config_user_override_severity() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "rust.msrv_defined".to_string(),
                severity: Severity::Error,
                enabled: true,
                triggers: vec![],
            });

            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Error);
        }

        #[test]
        fn effective_config_user_override_disabled() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "rust.msrv_defined".to_string(),
                severity: Severity::Warn,
                enabled: false,
                triggers: vec![],
            });

            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(!effective.enabled);
        }

        #[test]
        fn effective_config_profile_skip_can_be_overridden() {
            let mut config = Config::default(); // Oss profile skips tools.*
            config.checks.push(CheckConfig {
                id: "tools.checksums_file_exists".to_string(),
                severity: Severity::Error,
                enabled: true,
                triggers: vec![],
            });

            // Check that tools check is skipped by default in oss
            let default_effective =
                effective_check_config(&Config::default(), "tools.checksums_file_exists");
            assert!(!default_effective.enabled);

            // Check that user override enables it
            let overridden = effective_check_config(&config, "tools.checksums_file_exists");
            assert!(overridden.enabled);
            assert_eq!(overridden.severity, Severity::Error);
        }

        #[test]
        fn effective_config_team_profile_defaults() {
            let config = Config {
                profile: Profile::Team,
                ..Default::default()
            };

            // rust.toolchain_msrv_relation is error in team
            let effective = effective_check_config(&config, "rust.toolchain_msrv_relation");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Error);

            // tools.* are warn in team
            let effective = effective_check_config(&config, "tools.checksums_file_exists");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Warn);
        }

        #[test]
        fn effective_config_strict_profile_defaults() {
            let config = Config {
                profile: Profile::Strict,
                ..Default::default()
            };

            // All checks are error in strict
            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Error);

            let effective = effective_check_config(&config, "workspace.resolver_v2");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Error);
        }

        #[test]
        fn effective_config_strict_profile_can_be_softened() {
            let config = Config {
                profile: Profile::Strict,
                checks: vec![CheckConfig {
                    id: "rust.msrv_defined".to_string(),
                    severity: Severity::Warn,
                    enabled: true,
                    triggers: vec![],
                }],
                ..Default::default()
            };

            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Warn);
        }

        #[test]
        fn effective_config_unknown_check_uses_profile_default() {
            let config = Config::default();

            let effective = effective_check_config(&config, "unknown.future_check");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Warn); // Oss default for unknown
        }

        #[test]
        fn effective_config_default_is_enabled_error() {
            let default = EffectiveCheckConfig::default();
            assert!(default.enabled);
            assert_eq!(default.severity, Severity::Error);
        }

        #[test]
        fn effective_config_no_matching_override() {
            let mut config = Config::default();
            config.checks.push(CheckConfig {
                id: "rust.msrv_consistent".to_string(),
                severity: Severity::Info,
                enabled: true,
                triggers: vec![],
            });

            // Request a different check than the override
            let effective = effective_check_config(&config, "rust.msrv_defined");
            assert!(effective.enabled);
            assert_eq!(effective.severity, Severity::Warn); // Profile default
        }
    }
}
