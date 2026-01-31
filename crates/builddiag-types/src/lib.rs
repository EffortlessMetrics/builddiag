use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchemaId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunInfo {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepoDetected {
    pub is_workspace: bool,
    pub members: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RepoInfo {
    pub root: String,
    pub detected: RepoDetected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Inputs {
    pub cargo_root: Option<String>,
    pub rust_toolchain: Option<String>,
    pub tools_checksums: Option<String>,
    pub tools_manifest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckReport {
    pub id: String,
    pub status: CheckStatus,
    pub findings: Vec<Finding>,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummaryCounts {
    pub info: usize,
    pub warn: usize,
    pub error: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub counts: SummaryCounts,
    pub verdict: Verdict,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub schema: SchemaId,
    pub tool: ToolInfo,
    pub run: RunInfo,
    pub repo: RepoInfo,
    pub inputs: Inputs,
    pub checks: Vec<CheckReport>,
    pub summary: Summary,
}

// -----------------
// Config
// -----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Error,
    Warn,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MsrvSource {
    Workspace,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RelationToMsrv {
    Equals,
    AtLeast,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Defaults {
    #[serde(default = "Defaults::default_fail_on")]
    pub fail_on: FailOn,
    #[serde(default = "Defaults::default_out_dir")]
    pub out_dir: String,
    #[serde(default)]
    pub diff_aware: bool,
    #[serde(default = "Defaults::default_base")]
    pub base: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PathsConfig {
    #[serde(default = "PathsConfig::default_cargo_root")]
    pub cargo_root: String,
    #[serde(default = "PathsConfig::default_rust_toolchain")]
    pub rust_toolchain: String,
    #[serde(default = "PathsConfig::default_tools_checksums")]
    pub tools_checksums: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MsrvPolicy {
    #[serde(default = "MsrvPolicy::default_require_defined")]
    pub require_defined: bool,
    #[serde(default = "MsrvPolicy::default_source")]
    pub source: MsrvSource,
    #[serde(default)]
    pub allow_per_crate_override: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolchainPolicy {
    #[serde(default = "ToolchainPolicy::default_require_pinned")]
    pub require_pinned: bool,
    #[serde(default = "ToolchainPolicy::default_relation")]
    pub relation_to_msrv: RelationToMsrv,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChecksumsPolicy {
    #[serde(default = "ChecksumsPolicy::default_require_file")]
    pub require_file: bool,
    #[serde(default)]
    pub require_coverage: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Policy {
    #[serde(default)]
    pub msrv: MsrvPolicy,
    #[serde(default)]
    pub toolchain: ToolchainPolicy,
    #[serde(default)]
    pub checksums: ChecksumsPolicy,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            msrv: MsrvPolicy::default(),
            toolchain: ToolchainPolicy::default(),
            checksums: ChecksumsPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckConfig {
    pub id: String,
    #[serde(default = "CheckConfig::default_severity")]
    pub severity: Severity,
    #[serde(default = "CheckConfig::default_enabled")]
    pub enabled: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub checks: Vec<CheckConfig>,
    /// Optional arbitrary metadata for downstream tooling.
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            defaults: Defaults::default(),
            paths: PathsConfig::default(),
            policy: Policy::default(),
            checks: Vec::new(),
            meta: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn check_overrides(&self) -> BTreeMap<String, CheckConfig> {
        let mut map = BTreeMap::new();
        for c in &self.checks {
            map.insert(c.id.clone(), c.clone());
        }
        map
    }
}
