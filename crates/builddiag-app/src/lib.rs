//! Application orchestration for builddiag build contract validation.
//!
//! This crate provides the high-level orchestration layer that coordinates
//! repository loading, check execution, and report generation. It serves as
//! the bridge between the CLI and the underlying domain logic.
//!
//! # Key Functions
//!
//! - [`load_config`]: Load configuration from a TOML file
//! - [`run_checks`]: Execute all checks and produce a complete report
//! - [`compute_changed_files`]: Determine files changed between git refs for diff-aware mode
//!
//! # Report Generation
//!
//! The main entry point is [`run_checks`], which:
//! 1. Loads repository state from the target directory
//! 2. Runs all configured checks
//! 3. Collects and sorts findings
//! 4. Computes summary statistics
//! 5. Returns a complete [`Report`](builddiag_types::Report)

use anyhow::{Context, Result, anyhow};
use builddiag_checks::run_selected_checks;
use builddiag_domain::{
    build_sensor_verdict, determine_verdict, exit_code_for, finding_to_sensor,
    sort_findings_canonical, summarize,
};
use builddiag_render::{render_github_annotations, render_markdown};
#[cfg(feature = "cache")]
pub use builddiag_repo::CacheConfig;
#[cfg(not(feature = "cache"))]
use builddiag_repo::load_repo_state;
#[cfg(feature = "cache")]
use builddiag_repo::load_repo_state_cached;
use builddiag_types::{
    Artifact, Capability, CheckReport, Config, Finding, GitInfo, HostInfo, Report, RunInfo,
    SENSOR_REPORT_SCHEMA_V1, SensorReport, SensorRunInfo, Severity, ToolInfo, Verdict,
};
use camino::Utf8Path;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

pub const REPORT_SCHEMA_V1: &str = "builddiag.report.v1";

pub fn load_config(path: Option<&Utf8Path>) -> Result<Config> {
    match path {
        None => Ok(Config::default()),
        Some(p) => {
            let txt = fs::read_to_string(p).with_context(|| format!("read config {p}"))?;
            let cfg: Config = toml::from_str(&txt).with_context(|| format!("parse config {p}"))?;
            Ok(cfg)
        }
    }
}

pub fn compute_changed_files(
    root: &Utf8Path,
    base: &str,
    head: &str,
) -> Result<Option<BTreeSet<String>>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-only")
        .arg(format!("{base}...{head}"))
        .output();

    let Ok(out) = out else {
        return Ok(None);
    };
    if !out.status.success() {
        // If git fails (not a repo, no remotes, etc.), fail open.
        return Ok(None);
    }

    let txt = String::from_utf8_lossy(&out.stdout);
    let mut set = BTreeSet::new();
    for line in txt.lines() {
        let p = line.trim();
        if !p.is_empty() {
            set.insert(p.to_string());
        }
    }
    Ok(Some(set))
}

pub struct CheckRun {
    pub report: Report,
    pub markdown: String,
    pub annotations: Vec<String>,
    pub exit_code: i32,
}

/// Run checks and produce a report.
///
/// # Arguments
///
/// * `root` - Repository root path
/// * `config` - Build configuration
/// * `allow_all` - If true, run all checks even in diff-aware mode
/// * `changed_files` - Optional set of changed files for diff-aware mode
/// * `cache_config` - Optional cache configuration (requires `cache` feature)
#[cfg(feature = "cache")]
pub fn run_check(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    changed_files: Option<BTreeSet<String>>,
    cache_config: Option<&CacheConfig>,
) -> Result<CheckRun> {
    let start = Utc::now();

    let repo_state = load_repo_state_cached(root, config, changed_files, cache_config)?;

    run_check_inner(root, config, allow_all, repo_state, start)
}

/// Run checks and produce a report (without caching support).
#[cfg(not(feature = "cache"))]
pub fn run_check(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    changed_files: Option<BTreeSet<String>>,
) -> Result<CheckRun> {
    let start = Utc::now();

    let repo_state = load_repo_state(root, config, changed_files)?;

    run_check_inner(root, config, allow_all, repo_state, start)
}

/// Inner implementation of run_check shared between cached and non-cached versions.
fn run_check_inner(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    repo_state: builddiag_repo::RepoState,
    start: chrono::DateTime<Utc>,
) -> Result<CheckRun> {
    // Run checks and sort findings for deterministic output
    let mut checks = run_selected_checks(&repo_state, config, allow_all)?;
    for check in &mut checks {
        sort_findings_canonical(&mut check.findings);
    }

    // Flatten findings from all check reports
    let mut findings = Vec::new();
    for check in &checks {
        findings.extend(check.findings.clone());
    }
    sort_findings_canonical(&mut findings);

    let summary = summarize(&checks);
    let verdict = determine_verdict(&checks);
    let end = Utc::now();
    let duration_ms = (end - start).num_milliseconds().max(0) as u64;

    // Get git info if available
    let git_info = get_git_info(root);

    let report = Report {
        schema: REPORT_SCHEMA_V1.to_string(),
        tool: Some(ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        run: Some(RunInfo {
            started_at: start,
            ended_at: Some(end),
            duration_ms,
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            git: git_info,
        }),
        verdict,
        findings,
        summary: Some(summary),
        data: None,
    };

    let markdown = render_markdown(&report);
    let annotations = render_github_annotations(&report);
    let exit_code = exit_code_for(verdict, config.defaults.fail_on);

    Ok(CheckRun {
        report,
        markdown,
        annotations,
        exit_code,
    })
}

pub fn write_atomic(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("no parent dir for {path}"))?;
    fs::create_dir_all(parent).with_context(|| format!("create {parent}"))?;

    let tmp = parent.join(format!(".{}.tmp", path.file_name().unwrap_or("out")));
    fs::write(&tmp, bytes).with_context(|| format!("write {tmp}"))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {tmp} -> {path}"))?;
    Ok(())
}

pub fn write_outputs(out_json: &Utf8Path, out_md: Option<&Utf8Path>, run: &CheckRun) -> Result<()> {
    let json = serde_json::to_vec_pretty(&run.report)?;
    write_atomic(out_json, &json)?;

    out_md
        .map(|md_path| write_atomic(md_path, run.markdown.as_bytes()))
        .transpose()?;

    Ok(())
}

fn get_git_info(root: &Utf8Path) -> Option<GitInfo> {
    // Get current commit SHA
    let commit_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !commit_out.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&commit_out.stdout)
        .trim()
        .to_string();

    // Get current branch name
    let branch_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .ok();
    let branch = branch_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|b| b != "HEAD"); // detached HEAD

    // Check if working directory is dirty
    let status_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .output()
        .ok();
    let dirty = status_out
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    Some(GitInfo {
        commit,
        branch,
        dirty,
    })
}

// =============================================================================
// Sensor Report Support (sensor.report.v1) - Cockpit CI Governance
// =============================================================================

/// Build capabilities map based on repository state and configuration.
///
/// Tracks "No Green By Omission" - explicitly recording what features
/// were or weren't available during the run.
///
/// # Capability Keys
///
/// - `git`: Whether git information was available
/// - `config`: Whether configuration file was loaded
/// - `toolchain`: Whether rust-toolchain.toml was found
/// - `checksums`: Whether checksums file was found
/// - `diff_aware`: Whether diff-aware mode was used
pub fn build_capabilities(
    config: &Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
) -> BTreeMap<String, Capability> {
    build_capabilities_inner(
        config,
        git_info,
        has_toolchain,
        has_checksums,
        diff_aware_used,
        false,
    )
}

/// Build capabilities map, optionally including substrate capability.
pub fn build_capabilities_with_substrate(
    config: &Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
    substrate_used: bool,
) -> BTreeMap<String, Capability> {
    build_capabilities_inner(
        config,
        git_info,
        has_toolchain,
        has_checksums,
        diff_aware_used,
        substrate_used,
    )
}

fn build_capabilities_inner(
    config: &Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
    substrate_used: bool,
) -> BTreeMap<String, Capability> {
    let mut caps = BTreeMap::new();

    // Git capability
    if git_info.is_some() {
        caps.insert("git".to_string(), Capability::available());
    } else {
        caps.insert(
            "git".to_string(),
            Capability::unavailable("git repository not detected"),
        );
    }

    // Config capability (always available since we have defaults)
    caps.insert("config".to_string(), Capability::available());

    // Toolchain capability
    if has_toolchain {
        caps.insert("toolchain".to_string(), Capability::available());
    } else {
        caps.insert(
            "toolchain".to_string(),
            Capability::unavailable("rust-toolchain.toml not found"),
        );
    }

    // Checksums capability
    if has_checksums {
        caps.insert("checksums".to_string(), Capability::available());
    } else if !config.policy.checksums.require_file {
        caps.insert(
            "checksums".to_string(),
            Capability::skipped("checksums not required by config"),
        );
    } else {
        caps.insert(
            "checksums".to_string(),
            Capability::unavailable("checksums file not found"),
        );
    }

    // Diff-aware capability
    if diff_aware_used {
        caps.insert("diff_aware".to_string(), Capability::available());
    } else if config.defaults.diff_aware {
        caps.insert(
            "diff_aware".to_string(),
            Capability::unavailable("could not compute git diff"),
        );
    } else {
        caps.insert(
            "diff_aware".to_string(),
            Capability::skipped("diff-aware mode not enabled"),
        );
    }

    // Substrate capability
    if substrate_used {
        caps.insert("substrate".to_string(), Capability::available());
    }

    caps
}

/// Convert a builddiag Report to a SensorReport.
///
/// Transforms the builddiag-native report format to the sensor.report.v1
/// format compatible with Cockpit CI governance ecosystem.
pub fn report_to_sensor(
    report: &Report,
    checks: &[CheckReport],
    capabilities: BTreeMap<String, Capability>,
    artifacts: Vec<Artifact>,
) -> SensorReport {
    // Convert findings to sensor findings with fingerprints
    let sensor_findings = report
        .findings
        .iter()
        .map(|f| finding_to_sensor(f, None, None))
        .collect();

    // Build sensor run info with capabilities
    let sensor_run = report.run.as_ref().map(|run| SensorRunInfo {
        started_at: run.started_at,
        ended_at: run.ended_at,
        duration_ms: run.duration_ms,
        host: run.host.clone(),
        git: run.git.clone(),
        capabilities,
    });

    SensorReport {
        schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
        tool: report.tool.clone(),
        run: sensor_run,
        verdict: build_sensor_verdict(report.verdict, checks),
        findings: sensor_findings,
        artifacts,
        data: report.data.clone(),
    }
}

/// Create an error receipt when an internal error occurs.
///
/// In Cockpit mode, we need to produce a valid report even when the tool
/// fails internally. This creates a minimal report with the error information.
///
/// Uses canonical tool-error identity:
/// - `check_id = "tool.runtime"`
/// - `code = "runtime_error"`
/// - `severity = error`
/// - `verdict = Error` (maps to `fail` in sensor format)
pub fn create_error_receipt(started_at: DateTime<Utc>, error: &anyhow::Error) -> Report {
    let end = Utc::now();
    let duration_ms = (end - started_at).num_milliseconds().max(0) as u64;

    Report {
        schema: REPORT_SCHEMA_V1.to_string(),
        tool: Some(ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        run: Some(RunInfo {
            started_at,
            ended_at: Some(end),
            duration_ms,
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            git: None,
        }),
        verdict: Verdict::Error,
        findings: vec![Finding {
            check_id: "tool.runtime".to_string(),
            code: "runtime_error".to_string(),
            severity: Severity::Error,
            message: format!("Internal error: {error:#}"),
            location: None,
        }],
        summary: None,
        data: None,
    }
}

/// Extended check run result that includes sensor format data.
pub struct SensorCheckRun {
    /// Standard check run result
    pub check_run: CheckRun,
    /// Sensor format report
    pub sensor_report: SensorReport,
    /// Check reports for verdict building
    pub checks: Vec<CheckReport>,
}

/// Run checks and produce both builddiag and sensor format reports.
///
/// This is the main entry point for Cockpit-mode integration, producing
/// both the native builddiag report and the sensor.report.v1 format.
#[cfg(feature = "cache")]
pub fn run_check_with_sensor(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    changed_files: Option<BTreeSet<String>>,
    cache_config: Option<&CacheConfig>,
) -> Result<SensorCheckRun> {
    let start = Utc::now();
    let diff_aware_used = changed_files.is_some();

    let repo_state = load_repo_state_cached(root, config, changed_files, cache_config)?;

    run_check_with_sensor_inner(root, config, allow_all, repo_state, start, diff_aware_used)
}

/// Run checks and produce both builddiag and sensor format reports (without caching).
#[cfg(not(feature = "cache"))]
pub fn run_check_with_sensor(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    changed_files: Option<BTreeSet<String>>,
) -> Result<SensorCheckRun> {
    let start = Utc::now();
    let diff_aware_used = changed_files.is_some();

    let repo_state = load_repo_state(root, config, changed_files)?;

    run_check_with_sensor_inner(root, config, allow_all, repo_state, start, diff_aware_used)
}

/// Inner implementation of run_check_with_sensor.
fn run_check_with_sensor_inner(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    repo_state: builddiag_repo::RepoState,
    start: chrono::DateTime<Utc>,
    diff_aware_used: bool,
) -> Result<SensorCheckRun> {
    // Determine capability states from repo
    let has_toolchain = repo_state.toolchain.is_some();
    let has_checksums = repo_state.tools_checksums.is_some();

    // Run checks and sort findings for deterministic output
    let mut checks = run_selected_checks(&repo_state, config, allow_all)?;
    for check in &mut checks {
        sort_findings_canonical(&mut check.findings);
    }

    // Flatten findings from all check reports
    let mut findings = Vec::new();
    for check in &checks {
        findings.extend(check.findings.clone());
    }
    sort_findings_canonical(&mut findings);

    let summary = summarize(&checks);
    let verdict = determine_verdict(&checks);
    let end = Utc::now();
    let duration_ms = (end - start).num_milliseconds().max(0) as u64;

    // Get git info if available
    let git_info = get_git_info(root);

    let report = Report {
        schema: REPORT_SCHEMA_V1.to_string(),
        tool: Some(ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        run: Some(RunInfo {
            started_at: start,
            ended_at: Some(end),
            duration_ms,
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            git: git_info.clone(),
        }),
        verdict,
        findings,
        summary: Some(summary),
        data: None,
    };

    let markdown = render_markdown(&report);
    let annotations = render_github_annotations(&report);
    let exit_code = exit_code_for(verdict, config.defaults.fail_on);

    // Build capabilities and sensor report
    let capabilities = build_capabilities(
        config,
        git_info.as_ref(),
        has_toolchain,
        has_checksums,
        diff_aware_used,
    );

    let sensor_report = report_to_sensor(&report, &checks, capabilities, vec![]);

    Ok(SensorCheckRun {
        check_run: CheckRun {
            report,
            markdown,
            annotations,
            exit_code,
        },
        sensor_report,
        checks,
    })
}

/// Run checks using a pre-computed [`RepoState`] from a substrate.
///
/// This is the substrate bridge entry point: when the caller has already
/// built a `RepoState` (e.g. via [`builddiag_repo::repo_state_from_substrate`]),
/// this function runs checks and produces both report formats without any
/// filesystem discovery.
pub fn run_check_with_sensor_from_repo_state(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    repo_state: builddiag_repo::RepoState,
) -> Result<SensorCheckRun> {
    let start = Utc::now();
    let diff_aware_used = repo_state.changed_files.is_some();

    run_check_with_sensor_inner(root, config, allow_all, repo_state, start, diff_aware_used)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn create_minimal_repo() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn demo() {}").unwrap();
        (temp, root)
    }

    /// Tests for load_config function
    /// _Requirements: 2.6, 8.2, 8.3_
    mod load_config_tests {
        use super::*;

        #[test]
        fn load_config_with_none_returns_default_config() {
            let config = load_config(None).expect("should return default config");

            // Verify it's the default config
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Error);
            assert_eq!(config.defaults.out_dir, "artifacts/builddiag");
            assert!(!config.defaults.diff_aware);
            assert!(config.checks.is_empty());
            assert!(config.meta.is_empty());
        }

        #[test]
        fn load_config_with_valid_toml_parses_successfully() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("builddiag.toml");

            let toml_content = r#"
[defaults]
fail_on = "warn"
out_dir = "custom/output"
diff_aware = true
base = "origin/develop"
head = "feature-branch"

[paths]
cargo_root = "custom/Cargo.toml"
rust_toolchain = "custom/rust-toolchain.toml"

[policy.msrv]
require_defined = false
source = "any"

[policy.toolchain]
require_pinned = false
allow_nightly = true

[policy.checksums]
require_file = false

[[checks]]
id = "msrv_defined"
severity = "warn"
enabled = false
triggers = ["Cargo.toml"]

[meta]
custom_key = "custom_value"
"#;

            std::fs::write(&config_path, toml_content).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let config = load_config(Some(utf8_path)).expect("should parse valid TOML");

            // Verify defaults section
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Warn);
            assert_eq!(config.defaults.out_dir, "custom/output");
            assert!(config.defaults.diff_aware);
            assert_eq!(config.defaults.base, "origin/develop");
            assert_eq!(config.defaults.head, "feature-branch");

            // Verify paths section
            assert_eq!(config.paths.cargo_root, "custom/Cargo.toml");
            assert_eq!(config.paths.rust_toolchain, "custom/rust-toolchain.toml");

            // Verify policy section
            assert!(!config.policy.msrv.require_defined);
            assert_eq!(config.policy.msrv.source, builddiag_types::MsrvSource::Any);
            assert!(!config.policy.toolchain.require_pinned);
            assert!(config.policy.toolchain.allow_nightly);
            assert!(!config.policy.checksums.require_file);

            // Verify checks section
            assert_eq!(config.checks.len(), 1);
            let check = &config.checks[0];
            assert_eq!(check.id, "msrv_defined");
            assert_eq!(check.severity, builddiag_types::Severity::Warn);
            assert!(!check.enabled);
            assert_eq!(check.triggers, vec!["Cargo.toml"]);

            // Verify meta section
            assert_eq!(
                config.meta.get("custom_key"),
                Some(&"custom_value".to_string())
            );
        }

        #[test]
        fn load_config_with_minimal_valid_toml() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("minimal.toml");

            // Empty TOML should use all defaults
            let toml_content = "";
            std::fs::write(&config_path, toml_content).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let config = load_config(Some(utf8_path)).expect("should parse empty TOML");

            // Should have all default values
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Error);
            assert_eq!(config.defaults.out_dir, "artifacts/builddiag");
            assert!(config.checks.is_empty());
        }

        #[test]
        fn load_config_with_missing_file_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let nonexistent_path = temp_dir.path().join("nonexistent.toml");

            let utf8_path = Utf8Path::from_path(nonexistent_path.as_path())
                .expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about reading the config file
            assert!(err_msg.contains("read config") || err_msg.contains("nonexistent.toml"));
        }

        #[test]
        fn load_config_with_invalid_toml_returns_error_with_context() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("invalid.toml");

            // Invalid TOML syntax
            let invalid_toml = r#"
[defaults
fail_on = "error"
"#;
            std::fs::write(&config_path, invalid_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(err_msg.contains("parse config") || err_msg.contains("invalid.toml"));
        }

        #[test]
        fn load_config_with_invalid_enum_value_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("bad_enum.toml");

            // Valid TOML syntax but invalid enum value
            let invalid_enum_toml = r#"
[defaults]
fail_on = "invalid_value"
"#;
            std::fs::write(&config_path, invalid_enum_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(err_msg.contains("parse config") || err_msg.contains("bad_enum.toml"));
        }

        #[test]
        fn load_config_with_wrong_type_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("wrong_type.toml");

            // Valid TOML syntax but wrong type (string instead of bool)
            let wrong_type_toml = r#"
[defaults]
diff_aware = "yes"
"#;
            std::fs::write(&config_path, wrong_type_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(err_msg.contains("parse config") || err_msg.contains("wrong_type.toml"));
        }
    }

    /// Tests for write_atomic function
    /// _Requirements: 2.6_
    mod write_atomic_tests {
        use super::*;
        use camino::Utf8PathBuf;

        #[test]
        fn write_atomic_creates_parent_directories() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let nested_path = temp_dir.path().join("nested").join("deep").join("file.txt");
            let utf8_path = Utf8PathBuf::from_path_buf(nested_path.clone())
                .expect("path should be valid UTF-8");

            let content = b"test content";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok());
            assert!(nested_path.exists());

            let read_content = std::fs::read(&nested_path).expect("should read file");
            assert_eq!(read_content, content);
        }

        #[test]
        fn write_atomic_writes_content_correctly() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("output.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"Hello, World!\nThis is a test file.";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, content);
        }

        #[test]
        fn write_atomic_overwrites_existing_file() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("existing.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Write initial content
            let initial_content = b"initial content";
            std::fs::write(&file_path, initial_content).expect("should write initial file");

            // Overwrite with new content using write_atomic
            let new_content = b"new content that is different";
            let result = write_atomic(&utf8_path, new_content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, new_content);
        }

        #[test]
        fn write_atomic_handles_empty_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("empty.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert!(read_content.is_empty());
        }

        #[test]
        fn write_atomic_handles_binary_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("binary.bin");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Binary content with null bytes and various byte values
            let content: Vec<u8> = (0u8..=255).collect();
            let result = write_atomic(&utf8_path, &content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, content);
        }

        #[test]
        fn write_atomic_cleans_up_temp_file_on_success() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("cleanup_test.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"test content";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok());

            // Check that no .tmp file remains in the directory
            let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
                .expect("should read dir")
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
                .collect();

            assert!(entries.is_empty());
        }

        #[test]
        fn write_atomic_returns_error_for_root_path() {
            // A path with no parent (like "/" on Unix) should return an error
            let root_path = Utf8Path::new("/");
            let content = b"test";

            let result = write_atomic(root_path, content);

            assert!(result.is_err());
            let err_msg = format!("{:#}", result.unwrap_err());
            assert!(err_msg.contains("no parent dir"));
        }

        #[test]
        fn write_atomic_handles_large_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("large.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Create 1MB of content
            let content: Vec<u8> = vec![b'x'; 1024 * 1024];
            let result = write_atomic(&utf8_path, &content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content.len(), content.len());
            assert_eq!(read_content, content);
        }

        #[test]
        fn write_atomic_preserves_unicode_in_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("unicode.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = "Hello, 世界! 🦀 Rust is awesome! Ñoño".as_bytes();
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok());

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, content);
        }
    }

    /// Tests for write_outputs function
    mod write_outputs_tests {
        use super::*;
        use camino::Utf8PathBuf;

        #[test]
        fn write_outputs_writes_markdown_when_requested() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
                .expect("path should be valid UTF-8");
            let out_json = root.join("report.json");
            let out_md = root.join("comment.md");

            let run = CheckRun {
                report: Report {
                    schema: REPORT_SCHEMA_V1.to_string(),
                    tool: None,
                    run: None,
                    verdict: Verdict::Pass,
                    findings: Vec::new(),
                    summary: None,
                    data: None,
                },
                markdown: "markdown".to_string(),
                annotations: Vec::new(),
                exit_code: 0,
            };

            write_outputs(&out_json, Some(&out_md), &run).unwrap();

            assert!(out_json.exists());
            assert!(out_md.exists());
            let md = std::fs::read_to_string(&out_md).unwrap();
            assert_eq!(md, "markdown");
        }
    }

    /// Tests for sensor report support functions
    mod sensor_support_tests {
        use super::*;
        use builddiag_types::{CapabilityStatus, CheckStatus, Location, Summary, VerdictStatus};
        use std::collections::BTreeMap;

        #[test]
        fn build_capabilities_with_all_available() {
            let config = Config::default();
            let git = GitInfo {
                commit: "abc123".to_string(),
                branch: Some("main".to_string()),
                dirty: false,
            };

            let caps = build_capabilities(&config, Some(&git), true, true, true);

            assert_eq!(caps.get("git").unwrap().status, CapabilityStatus::Available);
            assert_eq!(
                caps.get("config").unwrap().status,
                CapabilityStatus::Available
            );
            assert_eq!(
                caps.get("toolchain").unwrap().status,
                CapabilityStatus::Available
            );
            assert_eq!(
                caps.get("checksums").unwrap().status,
                CapabilityStatus::Available
            );
            assert_eq!(
                caps.get("diff_aware").unwrap().status,
                CapabilityStatus::Available
            );
        }

        #[test]
        fn build_capabilities_with_none_available() {
            let mut config = Config::default();
            config.policy.checksums.require_file = true;
            config.defaults.diff_aware = true;

            let caps = build_capabilities(&config, None, false, false, false);

            assert_eq!(
                caps.get("git").unwrap().status,
                CapabilityStatus::Unavailable
            );
            assert!(caps.get("git").unwrap().reason.is_some());

            assert_eq!(
                caps.get("toolchain").unwrap().status,
                CapabilityStatus::Unavailable
            );
            assert_eq!(
                caps.get("checksums").unwrap().status,
                CapabilityStatus::Unavailable
            );
            assert_eq!(
                caps.get("diff_aware").unwrap().status,
                CapabilityStatus::Unavailable
            );
        }

        #[test]
        fn build_capabilities_with_skipped() {
            let mut config = Config::default();
            config.policy.checksums.require_file = false;
            config.defaults.diff_aware = false;

            let caps = build_capabilities(&config, None, false, false, false);

            assert_eq!(
                caps.get("checksums").unwrap().status,
                CapabilityStatus::Skipped
            );
            assert_eq!(
                caps.get("diff_aware").unwrap().status,
                CapabilityStatus::Skipped
            );
        }

        #[test]
        fn create_error_receipt_creates_valid_report() {
            let start = Utc::now();
            let error = anyhow!("Test error message");

            let receipt = create_error_receipt(start, &error);

            assert_eq!(receipt.schema, REPORT_SCHEMA_V1);
            assert_eq!(receipt.verdict, Verdict::Error);
            assert_eq!(receipt.findings.len(), 1);
            assert_eq!(receipt.findings[0].check_id, "tool.runtime");
            assert_eq!(receipt.findings[0].code, "runtime_error");
            assert_eq!(receipt.findings[0].severity, Severity::Error);
            assert!(receipt.findings[0].message.contains("Test error message"));
            assert!(receipt.tool.is_some());
            assert!(receipt.run.is_some());
        }

        #[test]
        fn report_to_sensor_converts_correctly() {
            let report = Report {
                schema: REPORT_SCHEMA_V1.to_string(),
                tool: Some(ToolInfo {
                    name: "builddiag".to_string(),
                    version: "0.1.0".to_string(),
                }),
                run: Some(RunInfo {
                    started_at: Utc::now(),
                    ended_at: Some(Utc::now()),
                    duration_ms: 100,
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
                    check_id: "test.check".to_string(),
                    code: "test_code".to_string(),
                    severity: Severity::Warn,
                    message: "Test warning".to_string(),
                    location: Some(Location {
                        path: "file.rs".to_string(),
                        line: Some(10),
                        col: None,
                    }),
                }],
                summary: Some(Summary {
                    total_findings: 1,
                    by_severity: BTreeMap::new(),
                    by_check: BTreeMap::new(),
                }),
                data: None,
            };

            let checks = vec![CheckReport {
                id: "test.check".to_string(),
                status: CheckStatus::Warn,
                findings: report.findings.clone(),
                skipped_reason: None,
                skipped_detail: None,
            }];

            let mut caps = BTreeMap::new();
            caps.insert("git".to_string(), Capability::available());

            let sensor = report_to_sensor(&report, &checks, caps, vec![]);

            assert_eq!(sensor.schema, SENSOR_REPORT_SCHEMA_V1);
            assert_eq!(sensor.verdict.status, VerdictStatus::Warn);
            assert_eq!(sensor.findings.len(), 1);
            assert!(!sensor.findings[0].fingerprint.is_empty());
            assert!(sensor.run.is_some());
            assert!(!sensor.run.as_ref().unwrap().capabilities.is_empty());
        }

        #[test]
        fn report_to_sensor_includes_artifacts() {
            let report = Report {
                schema: REPORT_SCHEMA_V1.to_string(),
                tool: None,
                run: None,
                verdict: Verdict::Pass,
                findings: vec![],
                summary: None,
                data: None,
            };

            let artifacts = vec![Artifact {
                name: "markdown".to_string(),
                path: "comment.md".to_string(),
                mime_type: Some("text/markdown".to_string()),
            }];

            let sensor = report_to_sensor(&report, &[], BTreeMap::new(), artifacts);

            assert_eq!(sensor.artifacts.len(), 1);
            assert_eq!(sensor.artifacts[0].name, "markdown");
        }

        #[test]
        fn build_capabilities_with_substrate_includes_flag() {
            let config = Config::default();
            let caps = build_capabilities_with_substrate(&config, None, false, false, false, true);
            assert!(caps.contains_key("substrate"));
        }
    }

    mod run_check_tests {
        use super::*;

        #[test]
        fn run_check_produces_report() {
            let (_temp, root) = create_minimal_repo();
            let config = Config::default();
            let cache = CacheConfig::default();

            let run = run_check(&root, &config, false, None, Some(&cache)).unwrap();
            assert_eq!(run.report.schema, REPORT_SCHEMA_V1);
            assert!(!run.markdown.is_empty());
        }

        #[test]
        fn run_check_with_sensor_produces_sensor_report() {
            let (_temp, root) = create_minimal_repo();
            let config = Config::default();
            let cache = CacheConfig::default();

            let run = run_check_with_sensor(&root, &config, false, None, Some(&cache)).unwrap();
            assert_eq!(run.sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
            assert_eq!(run.check_run.report.schema, REPORT_SCHEMA_V1);
        }
    }

    mod git_tests {
        use super::*;
        use std::process::Command;
        use std::sync::{Mutex, OnceLock};
        use tempfile::TempDir;

        static GIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        fn git_lock() -> std::sync::MutexGuard<'static, ()> {
            GIT_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
        }

        struct EnvGuard {
            key: &'static str,
            original: Option<String>,
        }

        impl EnvGuard {
            fn set(key: &'static str, value: &str) -> Self {
                let original = std::env::var(key).ok();
                unsafe {
                    std::env::set_var(key, value);
                }
                Self { key, original }
            }
        }

        impl Drop for EnvGuard {
            fn drop(&mut self) {
                if let Some(val) = &self.original {
                    unsafe {
                        std::env::set_var(self.key, val);
                    }
                } else {
                    unsafe {
                        std::env::remove_var(self.key);
                    }
                }
            }
        }

        fn run_git(root: &Utf8Path, args: &[&str]) {
            let status = Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .status()
                .expect("git should run");
            assert!(status.success());
        }

        #[test]
        fn compute_changed_files_returns_none_for_non_repo() {
            let _lock = git_lock();
            let temp_dir = TempDir::new().expect("temp dir");
            let root = Utf8Path::from_path(temp_dir.path()).unwrap();
            let changed = compute_changed_files(root, "HEAD", "HEAD").unwrap();
            assert!(changed.is_none());
        }

        #[test]
        fn compute_changed_files_returns_paths_for_repo() {
            let _lock = git_lock();
            let temp_dir = TempDir::new().expect("temp dir");
            let root = Utf8Path::from_path(temp_dir.path()).unwrap();

            run_git(root, &["init", "-b", "main"]);
            run_git(root, &["config", "user.email", "test@example.com"]);
            run_git(root, &["config", "user.name", "Test User"]);

            let file_path = root.join("file.txt");
            std::fs::write(&file_path, "one").unwrap();
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "init"]);

            std::fs::write(&file_path, "two").unwrap();
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "second"]);

            let changed = compute_changed_files(root, "HEAD~1", "HEAD")
                .unwrap()
                .expect("expected diff-aware set");
            assert!(changed.contains("file.txt"));
        }

        #[test]
        fn compute_changed_files_returns_empty_set_when_no_diff() {
            let _lock = git_lock();
            let temp_dir = TempDir::new().expect("temp dir");
            let root = Utf8Path::from_path(temp_dir.path()).unwrap();

            run_git(root, &["init", "-b", "main"]);
            run_git(root, &["config", "user.email", "test@example.com"]);
            run_git(root, &["config", "user.name", "Test User"]);

            let file_path = root.join("file.txt");
            std::fs::write(&file_path, "content").unwrap();
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "init"]);

            let changed = compute_changed_files(root, "HEAD", "HEAD")
                .unwrap()
                .expect("expected diff-aware set");
            assert!(changed.is_empty());
        }

        #[test]
        fn compute_changed_files_returns_none_when_git_missing() {
            let _lock = git_lock();
            let temp_dir = TempDir::new().expect("temp dir");
            let root = Utf8Path::from_path(temp_dir.path()).unwrap();
            let _guard = EnvGuard::set("PATH", "");

            let changed = compute_changed_files(root, "HEAD", "HEAD").unwrap();
            assert!(changed.is_none());
        }

        #[test]
        fn env_guard_removes_missing_var_on_drop() {
            let _lock = git_lock();
            let key = "BUILDDIAG_ENV_GUARD_TEST_UNSET";
            unsafe {
                std::env::remove_var(key);
            }

            {
                let _guard = EnvGuard::set(key, "value");
                assert_eq!(std::env::var(key).ok().as_deref(), Some("value"));
            }

            assert!(std::env::var(key).is_err());
        }

        #[test]
        fn get_git_info_reports_dirty_and_branch() {
            let _lock = git_lock();
            let temp_dir = TempDir::new().expect("temp dir");
            let root = Utf8Path::from_path(temp_dir.path()).unwrap();

            run_git(root, &["init", "-b", "main"]);
            run_git(root, &["config", "user.email", "test@example.com"]);
            run_git(root, &["config", "user.name", "Test User"]);

            let file_path = root.join("file.txt");
            std::fs::write(&file_path, "clean").unwrap();
            run_git(root, &["add", "."]);
            run_git(root, &["commit", "-m", "init"]);

            let info = super::get_git_info(root).expect("git info");
            assert!(info.commit.len() >= 7);
            assert_eq!(info.branch.as_deref(), Some("main"));
            assert!(!info.dirty);

            std::fs::write(&file_path, "dirty").unwrap();
            let dirty = super::get_git_info(root).expect("git info");
            assert!(dirty.dirty);
        }
    }
}
