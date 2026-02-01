//! Shared types for the builddiag build contract validator.
//!
//! This crate provides the core data structures used throughout builddiag:
//!
//! - [`Report`] - The main output structure containing all check results
//! - [`Config`] - Configuration schema for customizing check behavior
//! - [`Finding`] - Individual validation findings with severity and location
//! - [`CheckReport`] - Results from a single check execution
//! - [`Severity`] - Finding severity levels (Info, Warn, Error)
//! - [`CheckStatus`] - Check execution status (Pass, Warn, Fail, Skip)
//!
//! # Report Structure
//!
//! The [`Report`] type is the primary output of builddiag. It contains:
//! - Metadata about the tool and run
//! - Repository information
//! - Individual check results
//! - An overall summary with verdict
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
//! use builddiag_types::{Finding, Severity};
//!
//! let finding = Finding {
//!     severity: Severity::Error,
//!     code: "missing_msrv".to_string(),
//!     message: "Missing rust-version in Cargo.toml".to_string(),
//!     path: Some("Cargo.toml".to_string()),
//!     line: None,
//!     column: None,
//! };
//! ```

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Unique identifier for this run.
    pub id: String,
    /// Timestamp when the run started.
    pub started_at: DateTime<Utc>,
    /// Timestamp when the run completed, if finished.
    pub ended_at: Option<DateTime<Utc>>,
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
/// use builddiag_types::{Finding, Severity};
///
/// let finding = Finding {
///     severity: Severity::Error,
///     code: "missing_msrv".to_string(),
///     message: "Missing rust-version in Cargo.toml".to_string(),
///     path: Some("Cargo.toml".to_string()),
///     line: Some(1),
///     column: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// Severity level of this finding.
    pub severity: Severity,
    /// Machine-readable code identifying the type of finding.
    pub code: String,
    /// Human-readable description of the finding.
    pub message: String,
    /// File path where the finding was detected, if applicable.
    pub path: Option<String>,
    /// Line number in the file, if applicable.
    pub line: Option<u32>,
    /// Column number in the file, if applicable.
    pub column: Option<u32>,
}

/// Results from executing a single check.
///
/// Each check produces a report containing its status and any findings.
/// If the check was skipped, `skipped_reason` explains why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
/// an overall verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    /// Counts of findings by severity.
    pub counts: SummaryCounts,
    /// Overall verdict based on all check results.
    pub verdict: Verdict,
    /// Human-readable reasons explaining the verdict.
    pub reasons: Vec<String>,
}

/// The main report output from builddiag.
///
/// A report contains all information about a validation run, including
/// metadata, repository information, individual check results, and
/// an overall summary.
///
/// # Structure
///
/// - `schema` - Schema version for forward compatibility
/// - `tool` - Information about the builddiag version
/// - `run` - Execution metadata (timestamps, run ID)
/// - `repo` - Repository being analyzed
/// - `inputs` - Input files that were processed
/// - `checks` - Results from each check
/// - `summary` - Aggregated results and verdict
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    /// Schema identifier for this report format.
    pub schema: SchemaId,
    /// Information about the tool that generated this report.
    pub tool: ToolInfo,
    /// Information about this execution run.
    pub run: RunInfo,
    /// Information about the repository being analyzed.
    pub repo: RepoInfo,
    /// Paths to input files that were analyzed.
    pub inputs: Inputs,
    /// Results from each check that was executed.
    pub checks: Vec<CheckReport>,
    /// Summary of all check results.
    pub summary: Summary,
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

/// Combined policy settings for all check categories.
///
/// Groups together MSRV, toolchain, and checksums policies.
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
