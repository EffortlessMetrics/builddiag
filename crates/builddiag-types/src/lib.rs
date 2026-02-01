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

impl Profile {
    /// Returns the check state (severity or skip) for a given check ID under this profile.
    ///
    /// Profile severity mappings:
    /// - `oss`: msrv_defined=warn, msrv_consistent=error, toolchain_pinning=info,
    ///   resolver_v2=warn, checksums=skip
    /// - `team`: msrv_defined=warn, msrv_consistent=error, toolchain_pinning=warn,
    ///   resolver_v2=error, checksums=warn
    /// - `strict`: all checks at error severity
    pub fn check_state(&self, check_id: &str) -> ProfileCheckState {
        match self {
            Profile::Oss => match check_id {
                "rust.msrv_defined" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.msrv_consistent" => ProfileCheckState::Enabled(Severity::Error),
                "rust.toolchain_pinning" => ProfileCheckState::Enabled(Severity::Info),
                "rust.toolchain_msrv_relation" => ProfileCheckState::Enabled(Severity::Info),
                "tools.checksums_file_exists" => ProfileCheckState::Skip,
                "tools.checksums_format" => ProfileCheckState::Skip,
                "tools.checksums_coverage" => ProfileCheckState::Skip,
                "tools.checksums_verify_local" => ProfileCheckState::Skip,
                "workspace.resolver_v2" => ProfileCheckState::Enabled(Severity::Warn),
                _ => ProfileCheckState::Enabled(Severity::Warn),
            },
            Profile::Team => match check_id {
                "rust.msrv_defined" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.msrv_consistent" => ProfileCheckState::Enabled(Severity::Error),
                "rust.toolchain_pinning" => ProfileCheckState::Enabled(Severity::Warn),
                "rust.toolchain_msrv_relation" => ProfileCheckState::Enabled(Severity::Warn),
                "tools.checksums_file_exists" => ProfileCheckState::Enabled(Severity::Warn),
                "tools.checksums_format" => ProfileCheckState::Enabled(Severity::Warn),
                "tools.checksums_coverage" => ProfileCheckState::Enabled(Severity::Warn),
                "tools.checksums_verify_local" => ProfileCheckState::Enabled(Severity::Warn),
                "workspace.resolver_v2" => ProfileCheckState::Enabled(Severity::Error),
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
                severity: Severity::Error,
                code: "missing_msrv".to_string(),
                message: "Missing rust-version in Cargo.toml".to_string(),
                path: Some("Cargo.toml".to_string()),
                line: Some(1),
                column: Some(5),
            };

            assert_eq!(finding.severity, Severity::Error);
            assert_eq!(finding.code, "missing_msrv");
            assert_eq!(finding.message, "Missing rust-version in Cargo.toml");
            assert_eq!(finding.path, Some("Cargo.toml".to_string()));
            assert_eq!(finding.line, Some(1));
            assert_eq!(finding.column, Some(5));
        }

        #[test]
        fn finding_with_warn_severity() {
            let finding = Finding {
                severity: Severity::Warn,
                code: "msrv_mismatch".to_string(),
                message: "MSRV differs from toolchain version".to_string(),
                path: Some("rust-toolchain.toml".to_string()),
                line: None,
                column: None,
            };

            assert_eq!(finding.severity, Severity::Warn);
            assert_eq!(finding.code, "msrv_mismatch");
            assert_eq!(finding.path, Some("rust-toolchain.toml".to_string()));
            assert!(finding.line.is_none());
            assert!(finding.column.is_none());
        }

        #[test]
        fn finding_with_info_severity() {
            let finding = Finding {
                severity: Severity::Info,
                code: "workspace_detected".to_string(),
                message: "Workspace with 5 members detected".to_string(),
                path: None,
                line: None,
                column: None,
            };

            assert_eq!(finding.severity, Severity::Info);
            assert_eq!(finding.code, "workspace_detected");
            assert!(finding.path.is_none());
        }

        #[test]
        fn finding_without_location_info() {
            let finding = Finding {
                severity: Severity::Error,
                code: "general_error".to_string(),
                message: "A general error occurred".to_string(),
                path: None,
                line: None,
                column: None,
            };

            assert!(finding.path.is_none());
            assert!(finding.line.is_none());
            assert!(finding.column.is_none());
        }

        #[test]
        fn finding_with_partial_location_info() {
            let finding = Finding {
                severity: Severity::Warn,
                code: "partial_location".to_string(),
                message: "Finding with path but no line".to_string(),
                path: Some("src/lib.rs".to_string()),
                line: None,
                column: None,
            };

            assert_eq!(finding.path, Some("src/lib.rs".to_string()));
            assert!(finding.line.is_none());
            assert!(finding.column.is_none());
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
                severity: Severity::Error,
                code: "test".to_string(),
                message: "Test message".to_string(),
                path: Some("test.rs".to_string()),
                line: Some(10),
                column: Some(5),
            };

            let finding2 = Finding {
                severity: Severity::Error,
                code: "test".to_string(),
                message: "Test message".to_string(),
                path: Some("test.rs".to_string()),
                line: Some(10),
                column: Some(5),
            };

            assert_eq!(finding1, finding2);
        }

        #[test]
        fn finding_clone() {
            let finding = Finding {
                severity: Severity::Warn,
                code: "clone_test".to_string(),
                message: "Testing clone".to_string(),
                path: Some("file.rs".to_string()),
                line: Some(42),
                column: None,
            };

            let cloned = finding.clone();
            assert_eq!(finding, cloned);
        }
    }

    /// Tests for CheckReport construction with different statuses
    /// _Requirements: 2.1_
    mod check_report_tests {
        use super::*;

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
            let finding = Finding {
                severity: Severity::Warn,
                code: "msrv_mismatch".to_string(),
                message: "MSRV differs from toolchain".to_string(),
                path: Some("Cargo.toml".to_string()),
                line: None,
                column: None,
            };

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
            let finding = Finding {
                severity: Severity::Error,
                code: "missing_msrv".to_string(),
                message: "Missing rust-version".to_string(),
                path: Some("Cargo.toml".to_string()),
                line: Some(1),
                column: None,
            };

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
                Finding {
                    severity: Severity::Error,
                    code: "error1".to_string(),
                    message: "First error".to_string(),
                    path: Some("file1.rs".to_string()),
                    line: Some(10),
                    column: None,
                },
                Finding {
                    severity: Severity::Warn,
                    code: "warn1".to_string(),
                    message: "First warning".to_string(),
                    path: Some("file2.rs".to_string()),
                    line: Some(20),
                    column: None,
                },
                Finding {
                    severity: Severity::Info,
                    code: "info1".to_string(),
                    message: "First info".to_string(),
                    path: None,
                    line: None,
                    column: None,
                },
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

    /// Tests for Summary construction with counts and verdict
    /// _Requirements: 2.1_
    mod summary_tests {
        use super::*;

        #[test]
        fn summary_with_pass_verdict() {
            let summary = Summary {
                counts: SummaryCounts {
                    info: 0,
                    warn: 0,
                    error: 0,
                },
                verdict: Verdict::Pass,
                reasons: vec![],
            };

            assert_eq!(summary.verdict, Verdict::Pass);
            assert_eq!(summary.counts.info, 0);
            assert_eq!(summary.counts.warn, 0);
            assert_eq!(summary.counts.error, 0);
            assert!(summary.reasons.is_empty());
        }

        #[test]
        fn summary_with_warn_verdict() {
            let summary = Summary {
                counts: SummaryCounts {
                    info: 1,
                    warn: 2,
                    error: 0,
                },
                verdict: Verdict::Warn,
                reasons: vec!["2 warnings found".to_string()],
            };

            assert_eq!(summary.verdict, Verdict::Warn);
            assert_eq!(summary.counts.info, 1);
            assert_eq!(summary.counts.warn, 2);
            assert_eq!(summary.counts.error, 0);
            assert_eq!(summary.reasons.len(), 1);
        }

        #[test]
        fn summary_with_fail_verdict() {
            let summary = Summary {
                counts: SummaryCounts {
                    info: 2,
                    warn: 1,
                    error: 3,
                },
                verdict: Verdict::Fail,
                reasons: vec!["3 errors found".to_string(), "1 warning found".to_string()],
            };

            assert_eq!(summary.verdict, Verdict::Fail);
            assert_eq!(summary.counts.error, 3);
            assert_eq!(summary.reasons.len(), 2);
        }

        #[test]
        fn summary_with_skip_verdict() {
            let summary = Summary {
                counts: SummaryCounts {
                    info: 0,
                    warn: 0,
                    error: 0,
                },
                verdict: Verdict::Skip,
                reasons: vec!["All checks were skipped".to_string()],
            };

            assert_eq!(summary.verdict, Verdict::Skip);
            assert_eq!(summary.reasons, vec!["All checks were skipped".to_string()]);
        }

        #[test]
        fn summary_counts_construction() {
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
        fn summary_with_multiple_reasons() {
            let summary = Summary {
                counts: SummaryCounts {
                    info: 1,
                    warn: 2,
                    error: 1,
                },
                verdict: Verdict::Fail,
                reasons: vec![
                    "MSRV not defined".to_string(),
                    "Toolchain not pinned".to_string(),
                    "Checksums missing".to_string(),
                ],
            };

            assert_eq!(summary.reasons.len(), 3);
            assert!(summary.reasons.contains(&"MSRV not defined".to_string()));
            assert!(
                summary
                    .reasons
                    .contains(&"Toolchain not pinned".to_string())
            );
            assert!(summary.reasons.contains(&"Checksums missing".to_string()));
        }

        #[test]
        fn verdict_variants() {
            // Verify all Verdict variants can be constructed
            assert_eq!(Verdict::Pass, Verdict::Pass);
            assert_eq!(Verdict::Warn, Verdict::Warn);
            assert_eq!(Verdict::Fail, Verdict::Fail);
            assert_eq!(Verdict::Skip, Verdict::Skip);

            // Verify they are distinct
            assert_ne!(Verdict::Pass, Verdict::Fail);
            assert_ne!(Verdict::Warn, Verdict::Skip);
        }

        #[test]
        fn summary_equality() {
            let summary1 = Summary {
                counts: SummaryCounts {
                    info: 1,
                    warn: 2,
                    error: 3,
                },
                verdict: Verdict::Fail,
                reasons: vec!["test".to_string()],
            };

            let summary2 = Summary {
                counts: SummaryCounts {
                    info: 1,
                    warn: 2,
                    error: 3,
                },
                verdict: Verdict::Fail,
                reasons: vec!["test".to_string()],
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
            Report {
                schema: SchemaId("https://builddiag.dev/schema/report/v1".to_string()),
                tool: ToolInfo {
                    name: "builddiag".to_string(),
                    version: "0.1.0".to_string(),
                },
                run: RunInfo {
                    id: "run-123".to_string(),
                    started_at: Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
                    ended_at: Some(Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 5).unwrap()),
                },
                repo: RepoInfo {
                    root: "/home/user/project".to_string(),
                    detected: RepoDetected {
                        is_workspace: true,
                        members: 5,
                    },
                },
                inputs: Inputs {
                    cargo_root: Some("Cargo.toml".to_string()),
                    rust_toolchain: Some("rust-toolchain.toml".to_string()),
                    tools_checksums: None,
                    tools_manifest: None,
                },
                checks: vec![
                    CheckReport {
                        id: "msrv_defined".to_string(),
                        status: CheckStatus::Pass,
                        findings: vec![],
                        skipped_reason: None,
                    },
                    CheckReport {
                        id: "toolchain_pinned".to_string(),
                        status: CheckStatus::Warn,
                        findings: vec![Finding {
                            severity: Severity::Warn,
                            code: "toolchain_mismatch".to_string(),
                            message: "Toolchain version differs from MSRV".to_string(),
                            path: Some("rust-toolchain.toml".to_string()),
                            line: None,
                            column: None,
                        }],
                        skipped_reason: None,
                    },
                ],
                summary: Summary {
                    counts: SummaryCounts {
                        info: 0,
                        warn: 1,
                        error: 0,
                    },
                    verdict: Verdict::Warn,
                    reasons: vec!["1 warning found".to_string()],
                },
            }
        }

        #[test]
        fn report_construction_with_all_fields() {
            let report = create_test_report();

            assert_eq!(report.schema.0, "https://builddiag.dev/schema/report/v1");
            assert_eq!(report.tool.name, "builddiag");
            assert_eq!(report.tool.version, "0.1.0");
            assert_eq!(report.run.id, "run-123");
            assert!(report.run.ended_at.is_some());
            assert_eq!(report.repo.root, "/home/user/project");
            assert!(report.repo.detected.is_workspace);
            assert_eq!(report.repo.detected.members, 5);
            assert_eq!(report.checks.len(), 2);
            assert_eq!(report.summary.verdict, Verdict::Warn);
        }

        #[test]
        fn report_schema_id_construction() {
            let schema = SchemaId("test-schema-v1".to_string());
            assert_eq!(schema.0, "test-schema-v1");
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
                id: "unique-run-id".to_string(),
                started_at: started,
                ended_at: Some(ended),
            };

            assert_eq!(run.id, "unique-run-id");
            assert_eq!(run.started_at, started);
            assert_eq!(run.ended_at, Some(ended));
        }

        #[test]
        fn report_run_info_without_end_time() {
            let started = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();

            let run = RunInfo {
                id: "in-progress-run".to_string(),
                started_at: started,
                ended_at: None,
            };

            assert!(run.ended_at.is_none());
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
        fn report_repo_detected_workspace() {
            let detected = RepoDetected {
                is_workspace: true,
                members: 10,
            };

            assert!(detected.is_workspace);
            assert_eq!(detected.members, 10);
        }

        #[test]
        fn report_repo_detected_single_crate() {
            let detected = RepoDetected {
                is_workspace: false,
                members: 1,
            };

            assert!(!detected.is_workspace);
            assert_eq!(detected.members, 1);
        }

        #[test]
        fn report_inputs_all_present() {
            let inputs = Inputs {
                cargo_root: Some("Cargo.toml".to_string()),
                rust_toolchain: Some("rust-toolchain.toml".to_string()),
                tools_checksums: Some("scripts/tools.sha256".to_string()),
                tools_manifest: Some("scripts/tools.toml".to_string()),
            };

            assert!(inputs.cargo_root.is_some());
            assert!(inputs.rust_toolchain.is_some());
            assert!(inputs.tools_checksums.is_some());
            assert!(inputs.tools_manifest.is_some());
        }

        #[test]
        fn report_inputs_partial() {
            let inputs = Inputs {
                cargo_root: Some("Cargo.toml".to_string()),
                rust_toolchain: None,
                tools_checksums: None,
                tools_manifest: None,
            };

            assert!(inputs.cargo_root.is_some());
            assert!(inputs.rust_toolchain.is_none());
            assert!(inputs.tools_checksums.is_none());
            assert!(inputs.tools_manifest.is_none());
        }

        #[test]
        fn report_inputs_all_none() {
            let inputs = Inputs {
                cargo_root: None,
                rust_toolchain: None,
                tools_checksums: None,
                tools_manifest: None,
            };

            assert!(inputs.cargo_root.is_none());
            assert!(inputs.rust_toolchain.is_none());
            assert!(inputs.tools_checksums.is_none());
            assert!(inputs.tools_manifest.is_none());
        }

        #[test]
        fn report_with_empty_checks() {
            let report = Report {
                schema: SchemaId("v1".to_string()),
                tool: ToolInfo {
                    name: "builddiag".to_string(),
                    version: "0.1.0".to_string(),
                },
                run: RunInfo {
                    id: "run-1".to_string(),
                    started_at: Utc::now(),
                    ended_at: None,
                },
                repo: RepoInfo {
                    root: ".".to_string(),
                    detected: RepoDetected {
                        is_workspace: false,
                        members: 1,
                    },
                },
                inputs: Inputs {
                    cargo_root: None,
                    rust_toolchain: None,
                    tools_checksums: None,
                    tools_manifest: None,
                },
                checks: vec![],
                summary: Summary {
                    counts: SummaryCounts {
                        info: 0,
                        warn: 0,
                        error: 0,
                    },
                    verdict: Verdict::Skip,
                    reasons: vec!["No checks executed".to_string()],
                },
            };

            assert!(report.checks.is_empty());
            assert_eq!(report.summary.verdict, Verdict::Skip);
        }

        #[test]
        fn report_equality() {
            let report1 = create_test_report();
            let report2 = create_test_report();

            assert_eq!(report1, report2);
        }

        #[test]
        fn report_clone() {
            let report = create_test_report();
            let cloned = report.clone();

            assert_eq!(report, cloned);
        }
    }
}
