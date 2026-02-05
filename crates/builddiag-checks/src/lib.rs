//! Check implementations for builddiag build contract validation.
//!
//! This crate provides the core check implementations used by builddiag to validate
//! Rust project build contracts. Each check examines a specific aspect of the repository
//! configuration and produces findings with associated severities.
//!
//! # Available Checks
//!
//! - **MSRV checks**: Validate Minimum Supported Rust Version configuration
//! - **Toolchain checks**: Verify rust-toolchain.toml settings
//! - **Checksum checks**: Validate file integrity via SHA-256 checksums
//! - **Dependency checks**: Hygiene validation for Cargo.toml dependencies
//!
//! # Usage
//!
//! ```ignore
//! use builddiag_checks::{run_selected_checks, CHECK_DOCS};
//! use builddiag_repo::load_repo_state;
//! use builddiag_types::Config;
//!
//! let repo = load_repo_state(root)?;
//! let config = Config::default();
//! let reports = run_selected_checks(&repo, &config, None)?;
//! ```
//!
//! # Check Documentation
//!
//! Use [`CHECK_DOCS`] to access documentation for all available checks,
//! including descriptions, remediation help, and related finding codes.

use anyhow::{Context, Result, anyhow};
use builddiag_domain::{check_status_from_findings, parse_rust_version};
use builddiag_repo::{RepoState, maybe_parse_numeric_version};
use builddiag_types::{
    CheckConfig, CheckReport, CheckStatus, Config, Finding, Location, RelationToMsrv, Severity,
    effective_check_config,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;

/// Documentation for a check, used by the `explain` subcommand.
#[derive(Debug, Clone)]
pub struct CheckDocumentation {
    /// Check ID (e.g., "rust.msrv_defined").
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Detailed description of what the check validates.
    pub description: &'static str,
    /// Short help text for remediation.
    pub help: &'static str,
    /// Optional documentation URL.
    pub url: Option<&'static str>,
    /// Finding codes this check can produce.
    pub codes: &'static [&'static str],
}

/// Registry of check documentation.
pub static CHECK_DOCS: &[CheckDocumentation] = &[
    CheckDocumentation {
        id: "rust.msrv_defined",
        name: "MSRV Defined",
        description: "Validates that the Minimum Supported Rust Version (MSRV) is explicitly \
                      defined in Cargo.toml. MSRV helps users and CI systems know which Rust \
                      version is required to build your crate.",
        help: "Add `rust-version = \"1.XX.0\"` to your workspace Cargo.toml under \
               [workspace.package] or [package].",
        url: Some("https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field"),
        codes: &["missing_msrv", "invalid_msrv_defined"],
    },
    CheckDocumentation {
        id: "rust.msrv_consistent",
        name: "MSRV Consistent",
        description: "Validates that all workspace members have consistent MSRV values. \
                      Inconsistent MSRV across crates can cause confusing build failures.",
        help: "Ensure all crates either inherit from workspace.package.rust-version or \
               explicitly set the same rust-version.",
        url: Some("https://doc.rust-lang.org/cargo/reference/workspaces.html"),
        codes: &[
            "invalid_msrv",
            "missing_member_msrv",
            "invalid_member_msrv",
            "msrv_mismatch",
        ],
    },
    CheckDocumentation {
        id: "rust.toolchain_pinning",
        name: "Toolchain Pinning",
        description: "Validates that rust-toolchain.toml pins the Rust version to a specific \
                      release (e.g., \"1.75.0\") rather than a moving target like \"stable\".",
        help: "Create rust-toolchain.toml with `channel = \"1.XX.0\"` to pin the version.",
        url: Some("https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file"),
        codes: &[
            "missing_toolchain",
            "nightly_disallowed",
            "unpinned_channel",
            "invalid_toolchain_version",
        ],
    },
    CheckDocumentation {
        id: "rust.toolchain_msrv_relation",
        name: "Toolchain-MSRV Relation",
        description: "Validates that the pinned toolchain version matches or exceeds the MSRV. \
                      This ensures CI tests against the version users will actually use.",
        help: "Set your toolchain channel to match your MSRV, or configure \
               policy.toolchain.relation_to_msrv = \"at_least\" to allow newer toolchains.",
        url: None,
        codes: &["toolchain_msrv_mismatch"],
    },
    CheckDocumentation {
        id: "tools.checksums_file_exists",
        name: "Checksums File Exists",
        description: "Validates that the tools checksums file (scripts/tools.sha256) exists. \
                      This file contains SHA256 hashes for tool binaries to verify integrity.",
        help: "Create scripts/tools.sha256 with checksums in the format: \
               `<sha256hash>  <filepath>`",
        url: None,
        codes: &["missing_checksums"],
    },
    CheckDocumentation {
        id: "tools.checksums_format",
        name: "Checksums Format",
        description: "Validates that the checksums file has valid format: 64-character hex \
                      SHA256 hashes followed by file paths, no duplicates.",
        help: "Ensure each line follows the format: `<64-char-sha256>  <filepath>`. \
               Generate with: `sha256sum <file>`",
        url: None,
        codes: &["invalid_hash", "missing_path", "duplicate_path"],
    },
    CheckDocumentation {
        id: "tools.checksums_coverage",
        name: "Checksums Coverage",
        description: "Validates that all tool files listed in the tools manifest have \
                      corresponding checksum entries.",
        help: "Add missing checksums for all files listed in scripts/tools.toml.",
        url: None,
        codes: &["missing_checksum", "unexpected_checksum"],
    },
    CheckDocumentation {
        id: "tools.checksums_verify_local",
        name: "Checksums Verify Local",
        description: "Verifies that local tool files match their recorded checksums. \
                      Detects tampering or corruption of tool binaries.",
        help: "Re-download or regenerate tools with mismatched checksums, then update \
               scripts/tools.sha256.",
        url: None,
        codes: &["missing_tool_file", "hash_mismatch"],
    },
    CheckDocumentation {
        id: "workspace.resolver_v2",
        name: "Workspace Resolver v2",
        description: "Validates that Cargo workspaces use resolver version 2. Resolver v2 \
                      has better feature unification and is required for edition 2021+.",
        help: "Add `resolver = \"2\"` to your [workspace] section in Cargo.toml.",
        url: Some(
            "https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2",
        ),
        codes: &["resolver_not_v2"],
    },
    CheckDocumentation {
        id: "deps.wildcard_version",
        name: "No Wildcard Versions",
        description: "Validates that dependencies do not use wildcard version specifications (\"*\"). \
                      Wildcard versions are fragile and can cause unexpected breakage.",
        help: "Replace `foo = \"*\"` with a specific version like `foo = \"1.0\"`.",
        url: None,
        codes: &["wildcard_version"],
    },
    CheckDocumentation {
        id: "deps.path_missing_version",
        name: "Path Dependencies Have Version",
        description: "Validates that path dependencies also specify a version. Path-only \
                      dependencies cannot be published to crates.io.",
        help: "Add a version field: `foo = { path = \"../foo\", version = \"0.1\" }`.",
        url: None,
        codes: &["path_missing_version"],
    },
    CheckDocumentation {
        id: "deps.workspace_inheritance",
        name: "Workspace Inheritance",
        description: "Suggests using workspace dependency inheritance when a dependency is \
                      defined in workspace.dependencies.",
        help: "Use `foo.workspace = true` instead of duplicating the version.",
        url: None,
        codes: &["missing_workspace_inheritance"],
    },
    CheckDocumentation {
        id: "workspace.edition_consistent",
        name: "Edition Consistent",
        description: "Validates that all workspace members use the same Rust edition. \
                      Inconsistent editions across crates can cause confusing behavior differences.",
        help: "Ensure all crates either inherit from workspace.package.edition or \
               explicitly set the same edition.",
        url: Some("https://doc.rust-lang.org/edition-guide/"),
        codes: &[
            "invalid_workspace_edition",
            "missing_member_edition",
            "invalid_member_edition",
            "edition_mismatch",
        ],
    },
    CheckDocumentation {
        id: "workspace.member_ordering",
        name: "Member Ordering",
        description: "Validates that workspace members in [workspace.members] are sorted \
                      alphabetically. Sorted members improve readability and reduce merge conflicts.",
        help: "Sort the members array alphabetically in Cargo.toml.",
        url: None,
        codes: &["members_not_sorted"],
    },
    CheckDocumentation {
        id: "deps.lockfile_present",
        name: "Lockfile Present",
        description: "Validates that Cargo.lock exists for binary crates. \
                      A lockfile ensures reproducible builds for applications.",
        help: "Run `cargo build` to generate Cargo.lock and commit it to version control.",
        url: Some(
            "https://doc.rust-lang.org/cargo/faq.html#why-do-binaries-have-cargolock-in-version-control-but-not-libraries",
        ),
        codes: &[
            "missing_lockfile_for_binary",
            "unexpected_lockfile_for_library",
        ],
    },
    CheckDocumentation {
        id: "workspace.publish_ready",
        name: "Publish Ready",
        description: "Validates that publishable crates have required metadata for crates.io. \
                      Required fields include description and license (or license-file). \
                      Recommended fields include repository, documentation, and keywords.",
        help: "Add the missing metadata fields to your Cargo.toml [package] section.",
        url: Some("https://doc.rust-lang.org/cargo/reference/manifest.html#the-package-section"),
        codes: &[
            "missing_description",
            "missing_license",
            "missing_repository",
            "missing_documentation",
            "missing_readme",
        ],
    },
    CheckDocumentation {
        id: "rust.edition_deprecations",
        name: "Edition Deprecations",
        description: "Warns about deprecated edition features and migration opportunities. \
                      Older editions may have deprecated syntax or missing modern features.",
        help: "Consider migrating to a newer Rust edition using `cargo fix --edition`.",
        url: Some("https://doc.rust-lang.org/edition-guide/"),
        codes: &["deprecated_edition", "edition_migration_available"],
    },
    CheckDocumentation {
        id: "deps.duplicate_versions",
        name: "Duplicate Dependency Versions",
        description: "Detects when the same dependency is specified with different versions \
                      across workspace members. This can lead to larger binaries and \
                      potential compatibility issues.",
        help: "Unify dependency versions using [workspace.dependencies] inheritance.",
        url: Some(
            "https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table",
        ),
        codes: &["duplicate_dependency_version"],
    },
    CheckDocumentation {
        id: "deps.security_advisory",
        name: "Security Advisory",
        description: "Checks dependencies against the RustSec advisory database for known \
                      security vulnerabilities. Requires the 'security' feature to be enabled.",
        help: "Update affected dependencies to patched versions or review advisories for mitigations.",
        url: Some("https://rustsec.org/"),
        codes: &[
            "security_vulnerability",
            "security_unmaintained",
            "security_yanked",
        ],
    },
];

/// Look up documentation for a check ID or finding code.
///
/// Returns `Some(CheckDocumentation)` if found, `None` otherwise.
pub fn explain_check(check_or_code: &str) -> Option<&'static CheckDocumentation> {
    // First try exact match on check ID
    if let Some(doc) = CHECK_DOCS.iter().find(|d| d.id == check_or_code) {
        return Some(doc);
    }

    // Then try finding by code
    CHECK_DOCS.iter().find(|d| d.codes.contains(&check_or_code))
}

pub struct CheckDef {
    pub id: &'static str,
    pub default_severity: Severity,
    pub default_triggers: &'static [&'static str],
}

pub const BUILTIN_CHECKS: &[CheckDef] = &[
    CheckDef {
        id: "rust.msrv_defined",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "rust.msrv_consistent",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "rust.toolchain_pinning",
        default_severity: Severity::Error,
        default_triggers: &["rust-toolchain", "rust-toolchain.toml"],
    },
    CheckDef {
        id: "rust.toolchain_msrv_relation",
        default_severity: Severity::Error,
        default_triggers: &[
            "rust-toolchain",
            "rust-toolchain.toml",
            "Cargo.toml",
            "**/Cargo.toml",
        ],
    },
    CheckDef {
        id: "tools.checksums_file_exists",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "tools.checksums_format",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "tools.checksums_coverage",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256", "scripts/tools.toml"],
    },
    CheckDef {
        id: "tools.checksums_verify_local",
        default_severity: Severity::Warn,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "workspace.resolver_v2",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml"],
    },
    CheckDef {
        id: "deps.wildcard_version",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "deps.path_missing_version",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "deps.workspace_inheritance",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "workspace.edition_consistent",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "workspace.member_ordering",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml"],
    },
    CheckDef {
        id: "deps.lockfile_present",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.lock", "Cargo.toml"],
    },
    CheckDef {
        id: "workspace.publish_ready",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "rust.edition_deprecations",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "deps.duplicate_versions",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml", "Cargo.lock"],
    },
    CheckDef {
        id: "deps.security_advisory",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.lock", "Cargo.toml"],
    },
];

/// Information needed to run a single check.
struct CheckTask<'a> {
    def: &'a CheckDef,
    effective_severity: Severity,
    skip_reason: Option<String>,
}

/// Prepare check tasks by evaluating which checks should run.
fn prepare_check_tasks<'a>(
    config: &Config,
    changed_files: Option<&BTreeSet<String>>,
    allow_all: bool,
) -> Vec<CheckTask<'a>> {
    let overrides = config.check_overrides();
    let profile = config.profile;

    BUILTIN_CHECKS
        .iter()
        .map(|def| {
            let ov = overrides.get(def.id);
            let effective = effective_check_config(config, def.id);

            // Check if disabled
            if !effective.enabled {
                return CheckTask {
                    def,
                    effective_severity: effective.severity,
                    skip_reason: Some(if ov.is_some() {
                        "disabled by config".to_string()
                    } else {
                        format!("disabled by {} profile", profile)
                    }),
                };
            }

            // Check if triggered by changed files
            let triggers = effective_triggers(def, ov);
            let should_run_check = if allow_all {
                true
            } else {
                should_run(changed_files, &triggers)
            };

            if !should_run_check {
                return CheckTask {
                    def,
                    effective_severity: effective.severity,
                    skip_reason: Some("diff-aware: no matching changed files".to_string()),
                };
            }

            CheckTask {
                def,
                effective_severity: effective.severity,
                skip_reason: None,
            }
        })
        .collect()
}

/// Execute a single check and return its report.
fn execute_check(task: &CheckTask, repo: &RepoState, config: &Config) -> Result<CheckReport> {
    // If the check should be skipped, return a skip report
    if let Some(reason) = &task.skip_reason {
        return Ok(CheckReport {
            id: task.def.id.to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(reason.clone()),
        });
    }

    let severity = task.effective_severity;

    let mut report = match task.def.id {
        "rust.msrv_defined" => check_msrv_defined(repo, config, severity)?,
        "rust.msrv_consistent" => check_msrv_consistent(repo, config, severity)?,
        "rust.toolchain_pinning" => check_toolchain_pinning(repo, config, severity)?,
        "rust.toolchain_msrv_relation" => check_toolchain_msrv_relation(repo, config, severity)?,
        "tools.checksums_file_exists" => check_checksums_file_exists(repo, config, severity)?,
        "tools.checksums_format" => check_checksums_format(repo, config, severity)?,
        "tools.checksums_coverage" => check_checksums_coverage(repo, config, severity)?,
        "tools.checksums_verify_local" => check_checksums_verify_local(repo, config, severity)?,
        "workspace.resolver_v2" => check_workspace_resolver(repo, config, severity)?,
        "deps.wildcard_version" => check_deps_wildcard(repo, config, severity)?,
        "deps.path_missing_version" => check_deps_path_version(repo, config, severity)?,
        "deps.workspace_inheritance" => check_deps_workspace_inheritance(repo, config, severity)?,
        "workspace.edition_consistent" => check_edition_consistent(repo, config, severity)?,
        "workspace.member_ordering" => check_member_ordering(repo, config, severity)?,
        "deps.lockfile_present" => check_lockfile_present(repo, config, severity)?,
        "workspace.publish_ready" => check_publish_ready(repo, config, severity)?,
        "rust.edition_deprecations" => check_edition_deprecations(repo, config, severity)?,
        "deps.duplicate_versions" => check_duplicate_versions(repo, config, severity)?,
        "deps.security_advisory" => check_security_advisory(repo, config, severity)?,
        _ => return Err(anyhow!("unknown check id: {}", task.def.id)),
    };

    report.status = check_status_from_findings(&report.findings);
    Ok(report)
}

/// Run selected checks sequentially (fallback when parallel feature is disabled).
#[cfg(not(feature = "parallel"))]
pub fn run_selected_checks(
    repo: &RepoState,
    config: &Config,
    allow_all: bool,
) -> Result<Vec<CheckReport>> {
    let tasks = prepare_check_tasks(config, repo.changed_files.as_ref(), allow_all);

    let mut reports: Vec<CheckReport> = tasks
        .iter()
        .map(|task| execute_check(task, repo, config))
        .collect::<Result<Vec<_>>>()?;

    // Sort reports by check ID for deterministic output
    reports.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(reports)
}

/// Run selected checks in parallel using rayon.
///
/// Checks are executed concurrently since they only read repo state and don't modify it.
/// Results are sorted by check ID after execution to ensure deterministic output.
#[cfg(feature = "parallel")]
pub fn run_selected_checks(
    repo: &RepoState,
    config: &Config,
    allow_all: bool,
) -> Result<Vec<CheckReport>> {
    let tasks = prepare_check_tasks(config, repo.changed_files.as_ref(), allow_all);

    // Execute checks in parallel
    let results: Vec<Result<CheckReport>> = tasks
        .par_iter()
        .map(|task| execute_check(task, repo, config))
        .collect();

    // Collect results, propagating any errors
    let mut reports: Vec<CheckReport> = results.into_iter().collect::<Result<Vec<_>>>()?;

    // Sort reports by check ID for deterministic output
    reports.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(reports)
}

fn effective_triggers(def: &CheckDef, ov: Option<&CheckConfig>) -> Vec<String> {
    if let Some(ov) = ov
        && !ov.triggers.is_empty()
    {
        return ov.triggers.clone();
    }
    def.default_triggers.iter().map(|s| s.to_string()).collect()
}

fn should_run(changed: Option<&BTreeSet<String>>, triggers: &[String]) -> bool {
    let Some(changed) = changed else {
        return true;
    };
    if triggers.is_empty() {
        return true;
    }

    let globset = build_globset(triggers).ok();
    let Some(globset) = globset else {
        // If triggers are invalid globs, fail open to avoid surprise skips.
        return true;
    };

    changed.iter().any(|p| globset.is_match(p))
}

fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g)?);
    }
    Ok(b.build()?)
}

fn mk_location(path: Option<String>, line: Option<u32>) -> Option<Location> {
    path.map(|p| Location {
        path: p,
        line,
        col: None,
    })
}

fn mk_finding(
    severity: Severity,
    check_id: &str,
    code: &str,
    message: impl Into<String>,
    path: Option<String>,
    line: Option<u32>,
) -> Finding {
    Finding {
        check_id: check_id.to_string(),
        code: code.to_string(),
        severity,
        message: message.into(),
        location: mk_location(path, line),
    }
}

fn check_msrv_defined(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let msrv = repo.workspace.workspace_msrv.clone();

    match config.policy.msrv.source {
        builddiag_types::MsrvSource::Workspace => {
            if let Some(ref msrv_raw) = msrv {
                // Validate that MSRV is parseable
                if parse_rust_version(msrv_raw).is_err() {
                    let path = repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p));
                    findings.push(mk_finding(
                        default_sev,
                        "rust.msrv_defined",
                        "invalid_msrv_defined",
                        format!("Invalid rust-version (MSRV) in Cargo.toml: '{msrv_raw}'"),
                        path,
                        None,
                    ));
                }
            } else if config.policy.msrv.require_defined {
                let path = repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p));
                findings.push(mk_finding(
                    default_sev,
                    "rust.msrv_defined",
                    "missing_msrv",
                    "Missing workspace/package rust-version (MSRV) in Cargo.toml",
                    path,
                    None,
                ));
            }
        }
        builddiag_types::MsrvSource::Any => {
            let mut any_msrv: Option<String> = msrv.clone();
            if any_msrv.is_none() {
                for m in &repo.workspace.members {
                    if m.rust_version.is_some() {
                        any_msrv = m.rust_version.clone();
                        break;
                    }
                }
            }
            if let Some(ref msrv_raw) = any_msrv {
                // Validate that MSRV is parseable
                if parse_rust_version(msrv_raw).is_err() {
                    let path = repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p));
                    findings.push(mk_finding(
                        default_sev,
                        "rust.msrv_defined",
                        "invalid_msrv_defined",
                        format!("Invalid rust-version (MSRV): '{msrv_raw}'"),
                        path,
                        None,
                    ));
                }
            } else if config.policy.msrv.require_defined {
                findings.push(mk_finding(
                    default_sev,
                    "rust.msrv_defined",
                    "missing_msrv",
                    "Missing rust-version (MSRV): define workspace.package.rust-version or per-crate package.rust-version",
                    repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                    None,
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "rust.msrv_defined".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_msrv_consistent(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let Some(workspace_msrv_raw) = repo.workspace.workspace_msrv.clone() else {
        return Ok(CheckReport {
            id: "rust.msrv_consistent".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no workspace/package MSRV to compare".to_string()),
        });
    };

    let workspace_msrv = match parse_rust_version(&workspace_msrv_raw) {
        Ok(v) => v.to_string(),
        Err(_) => {
            findings.push(mk_finding(
                default_sev,
                "rust.msrv_consistent",
                "invalid_msrv",
                format!("Invalid workspace MSRV rust-version: {workspace_msrv_raw}"),
                repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                None,
            ));
            return Ok(CheckReport {
                id: "rust.msrv_consistent".to_string(),
                status: check_status_from_findings(&findings),
                findings,
                skipped_reason: None,
            });
        }
    };

    let allowlist: HashSet<String> = config.policy.msrv.allow_overrides.iter().cloned().collect();

    for m in &repo.workspace.members {
        let rel = rel_path(&repo.root, &m.manifest_path);

        let effective = if let Some(rv) = &m.rust_version {
            Some(rv.clone())
        } else if m.rust_version_workspace {
            Some(workspace_msrv.clone())
        } else {
            None
        };

        let Some(effective_raw) = effective else {
            findings.push(mk_finding(
                default_sev,
                "rust.msrv_consistent",
                "missing_member_msrv",
                format!(
                    "{}: missing package.rust-version (and not set to inherit from workspace)",
                    m.name
                ),
                Some(rel),
                None,
            ));
            continue;
        };

        let effective_norm = match parse_rust_version(&effective_raw) {
            Ok(v) => v.to_string(),
            Err(_) => {
                findings.push(mk_finding(
                    default_sev,
                    "rust.msrv_consistent",
                    "invalid_member_msrv",
                    format!("{}: invalid rust-version '{effective_raw}'", m.name),
                    Some(rel),
                    None,
                ));
                continue;
            }
        };

        if effective_norm != workspace_msrv {
            let allowed = config.policy.msrv.allow_per_crate_override || allowlist.contains(&rel);
            if !allowed {
                findings.push(mk_finding(
                    default_sev,
                    "rust.msrv_consistent",
                    "msrv_mismatch",
                    format!("{}: rust-version {effective_norm} does not match workspace MSRV {workspace_msrv}", m.name),
                    Some(rel),
                    None,
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "rust.msrv_consistent".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_toolchain_pinning(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();

    let Some(tc) = &repo.toolchain else {
        if config.policy.toolchain.require_pinned {
            findings.push(mk_finding(
                default_sev,
                "rust.toolchain_pinning",
                "missing_toolchain",
                "Missing rust-toolchain.toml (or rust-toolchain) at repo root",
                None,
                None,
            ));
        }
        return Ok(CheckReport {
            id: "rust.toolchain_pinning".to_string(),
            status: check_status_from_findings(&findings),
            findings,
            skipped_reason: None,
        });
    };

    let channel = tc.channel.trim();

    if channel.eq_ignore_ascii_case("nightly") && !config.policy.toolchain.allow_nightly {
        findings.push(mk_finding(
            default_sev,
            "rust.toolchain_pinning",
            "nightly_disallowed",
            "Toolchain channel is 'nightly' but policy disallows nightly",
            Some(rel_path(&repo.root, &tc.path)),
            None,
        ));
    }

    if config.policy.toolchain.require_pinned {
        if channel.eq_ignore_ascii_case("stable")
            || channel.eq_ignore_ascii_case("beta")
            || channel.eq_ignore_ascii_case("nightly")
        {
            findings.push(mk_finding(
                default_sev,
                "rust.toolchain_pinning",
                "unpinned_channel",
                format!("Toolchain channel '{channel}' is not pinned to a specific version"),
                Some(rel_path(&repo.root, &tc.path)),
                None,
            ));
        } else if maybe_parse_numeric_version(channel)?.is_none() {
            findings.push(mk_finding(
                default_sev,
                "rust.toolchain_pinning",
                "invalid_toolchain_version",
                format!("Toolchain channel '{channel}' is not a valid numeric Rust version"),
                Some(rel_path(&repo.root, &tc.path)),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: "rust.toolchain_pinning".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_toolchain_msrv_relation(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let Some(tc) = &repo.toolchain else {
        return Ok(CheckReport {
            id: "rust.toolchain_msrv_relation".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no toolchain file".to_string()),
        });
    };
    let Some(msrv_raw) = &repo.workspace.workspace_msrv else {
        return Ok(CheckReport {
            id: "rust.toolchain_msrv_relation".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no workspace/package MSRV".to_string()),
        });
    };

    let toolchain_ver = match maybe_parse_numeric_version(&tc.channel)? {
        Some(v) => v,
        None => {
            return Ok(CheckReport {
                id: "rust.toolchain_msrv_relation".to_string(),
                status: CheckStatus::Skip,
                findings: Vec::new(),
                skipped_reason: Some("non-numeric toolchain channel".to_string()),
            });
        }
    };

    let msrv_ver = parse_rust_version(msrv_raw)
        .map_err(|e| anyhow!("invalid MSRV '{msrv_raw}': {e}"))?
        .to_string();

    let tc_v = parse_rust_version(&toolchain_ver)?;
    let ms_v = parse_rust_version(&msrv_ver)?;

    let ok = match config.policy.toolchain.relation_to_msrv {
        RelationToMsrv::Equals => tc_v == ms_v,
        RelationToMsrv::AtLeast => tc_v >= ms_v,
    };

    if !ok {
        let relation = match config.policy.toolchain.relation_to_msrv {
            RelationToMsrv::Equals => "must equal",
            RelationToMsrv::AtLeast => "must be at least",
        };
        findings.push(mk_finding(
            default_sev,
            "rust.toolchain_msrv_relation",
            "toolchain_msrv_mismatch",
            format!("Toolchain ({}) {relation} MSRV ({})", tc_v, ms_v),
            Some(rel_path(&repo.root, &tc.path)),
            None,
        ));
    }

    Ok(CheckReport {
        id: "rust.toolchain_msrv_relation".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_file_exists(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();
    if repo.tools_checksums.is_none() && config.policy.checksums.require_file {
        findings.push(mk_finding(
            default_sev,
            "tools.checksums_file_exists",
            "missing_checksums",
            "Missing scripts/tools.sha256",
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    Ok(CheckReport {
        id: "tools.checksums_file_exists".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_format(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_format".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let mut findings = Vec::new();
    let mut seen_paths = HashSet::new();

    for e in &cks.entries {
        let rel = rel_path(&repo.root, &cks.path);

        // Hash must be 64 hex chars.
        if e.hash.len() != 64 || hex::decode(&e.hash).is_err() {
            findings.push(mk_finding(
                default_sev,
                "tools.checksums_format",
                "invalid_hash",
                format!("Invalid sha256 hash for path '{}': '{}'", e.path, e.hash),
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        }

        if e.path.trim().is_empty() {
            findings.push(mk_finding(
                default_sev,
                "tools.checksums_format",
                "missing_path",
                "Checksum line missing path",
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        } else if !seen_paths.insert(e.path.clone()) {
            findings.push(mk_finding(
                default_sev,
                "tools.checksums_format",
                "duplicate_path",
                format!("Duplicate checksum entry for path '{}'", e.path),
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        }
    }

    Ok(CheckReport {
        id: "tools.checksums_format".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_coverage(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    if !config.policy.checksums.require_coverage {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("coverage not required by policy".to_string()),
        });
    }

    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let Some((_manifest_path, manifest)) = &repo.tools_manifest else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no tools manifest".to_string()),
        });
    };

    let have: HashSet<String> = cks.entries.iter().map(|e| e.path.clone()).collect();
    let mut expected = HashSet::new();
    for t in &manifest.tool {
        for f in &t.files {
            expected.insert(f.clone());
        }
    }

    let mut findings = Vec::new();

    for f in expected.difference(&have) {
        findings.push(mk_finding(
            default_sev,
            "tools.checksums_coverage",
            "missing_checksum",
            format!("Missing checksum entry for expected tool file '{}'", f),
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    // extras are usually benign; mark warn by default, still overrideable.
    for f in have.difference(&expected) {
        findings.push(mk_finding(
            Severity::Warn,
            "tools.checksums_coverage",
            "unexpected_checksum",
            format!(
                "Checksum contains entry not present in tools manifest: '{}'",
                f
            ),
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    Ok(CheckReport {
        id: "tools.checksums_coverage".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_verify_local(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    if !config.policy.checksums.verify_local_files {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("local verification not enabled".to_string()),
        });
    }
    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let mut findings = Vec::new();

    for e in &cks.entries {
        let p = repo.root.join(&e.path);
        if !p.exists() {
            findings.push(mk_finding(
                Severity::Warn,
                "tools.checksums_verify_local",
                "missing_tool_file",
                format!(
                    "Tool file '{}' not found on disk (skipping hash verify)",
                    e.path
                ),
                Some(e.path.clone()),
                None,
            ));
            continue;
        }

        let bytes = fs::read(&p).with_context(|| format!("read {}", p))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let got = format!("{:x}", hasher.finalize());
        let want = e.hash.to_ascii_lowercase();

        if got != want {
            findings.push(mk_finding(
                default_sev,
                "tools.checksums_verify_local",
                "hash_mismatch",
                format!(
                    "Hash mismatch for '{}': expected {}, got {}",
                    e.path, want, got
                ),
                Some(e.path.clone()),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: "tools.checksums_verify_local".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_workspace_resolver(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "workspace.resolver_v2";
    let mut findings = Vec::new();
    if !repo.workspace.is_workspace {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some("not a workspace".to_string()),
        });
    }

    let resolver = repo.workspace.workspace_resolver.as_deref();
    if resolver != Some("2") {
        findings.push(mk_finding(
            default_sev,
            CHECK_ID,
            "resolver_not_v2",
            format!("workspace.resolver is {:?}; expected '2'", resolver),
            Some("Cargo.toml".to_string()),
            None,
        ));
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_edition_consistent(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let mut findings = Vec::new();

    // Get workspace edition
    let Some(workspace_edition) = repo.workspace.workspace_edition.clone() else {
        return Ok(CheckReport {
            id: "workspace.edition_consistent".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no workspace edition to compare".to_string()),
        });
    };

    // Validate workspace edition is valid (2015, 2018, 2021, 2024)
    if !is_valid_edition(&workspace_edition) {
        findings.push(mk_finding(
            default_sev,
            "workspace.edition_consistent",
            "invalid_workspace_edition",
            format!("Invalid workspace edition: {workspace_edition}"),
            repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
            None,
        ));
        return Ok(CheckReport {
            id: "workspace.edition_consistent".to_string(),
            status: check_status_from_findings(&findings),
            findings,
            skipped_reason: None,
        });
    }

    let allowlist: HashSet<String> = config
        .policy
        .edition
        .allow_overrides
        .iter()
        .cloned()
        .collect();

    for m in &repo.workspace.members {
        let rel = rel_path(&repo.root, &m.manifest_path);

        // Get effective edition
        let effective = if let Some(ref ed) = m.edition {
            Some(ed.clone())
        } else if m.edition_workspace {
            Some(workspace_edition.clone())
        } else {
            None
        };

        let Some(effective_ed) = effective else {
            findings.push(mk_finding(
                default_sev,
                "workspace.edition_consistent",
                "missing_member_edition",
                format!(
                    "{}: missing package.edition (and not set to inherit from workspace)",
                    m.name
                ),
                Some(rel),
                None,
            ));
            continue;
        };

        if !is_valid_edition(&effective_ed) {
            findings.push(mk_finding(
                default_sev,
                "workspace.edition_consistent",
                "invalid_member_edition",
                format!("{}: invalid edition '{effective_ed}'", m.name),
                Some(rel),
                None,
            ));
            continue;
        }

        if effective_ed != workspace_edition {
            let allowed =
                config.policy.edition.allow_per_crate_override || allowlist.contains(&rel);
            if !allowed {
                findings.push(mk_finding(
                    default_sev,
                    "workspace.edition_consistent",
                    "edition_mismatch",
                    format!("{}: edition {effective_ed} does not match workspace edition {workspace_edition}", m.name),
                    Some(rel),
                    None,
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "workspace.edition_consistent".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn is_valid_edition(ed: &str) -> bool {
    matches!(ed, "2015" | "2018" | "2021" | "2024")
}

fn check_member_ordering(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "workspace.member_ordering";
    let mut findings = Vec::new();

    if !repo.workspace.is_workspace {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some("not a workspace".to_string()),
        });
    }

    if !config.policy.member_ordering.require_sorted {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some("sorting not required by policy".to_string()),
        });
    }

    // Get member patterns from workspace model if available, or compute on-demand
    let model = if let Some(ref m) = repo.workspace_model {
        Some(m.clone())
    } else if let Some(ref cargo_root) = repo.cargo_root {
        // Lazy compute the workspace model only when this check runs
        builddiag_repo::discover_workspace(cargo_root).ok()
    } else {
        None
    };

    let Some(model) = model else {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some("workspace model not available".to_string()),
        });
    };

    let patterns = &model.member_patterns;
    if patterns.is_empty() {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Pass,
            findings,
            skipped_reason: None,
        });
    }

    // Check if patterns are sorted
    let mut sorted = patterns.clone();
    sorted.sort();

    if patterns != &sorted {
        findings.push(mk_finding(
            default_sev,
            CHECK_ID,
            "members_not_sorted",
            "workspace.members is not sorted alphabetically".to_string(),
            Some("Cargo.toml".to_string()),
            None,
        ));
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn rel_path(root: &camino::Utf8Path, p: &camino::Utf8Path) -> String {
    p.strip_prefix(root).ok().unwrap_or(p).to_string()
}

fn check_deps_wildcard(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = depguard::Config {
        check_wildcards: true,
        check_path_version: false,
        check_workspace_inheritance: false,
        severity: depguard::Severity::Warn,
        ignore: Vec::new(),
    };

    let depguard_findings = depguard::check_workspace(&repo.root, &depguard_config)?;

    let findings: Vec<Finding> = depguard_findings
        .into_iter()
        .filter(|f| f.code == "wildcard_version")
        .map(|f| {
            mk_finding(
                default_sev,
                "deps.wildcard_version",
                &f.code,
                f.message,
                f.path,
                f.line,
            )
        })
        .collect();

    Ok(CheckReport {
        id: "deps.wildcard_version".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_deps_path_version(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = depguard::Config {
        check_wildcards: false,
        check_path_version: true,
        check_workspace_inheritance: false,
        severity: depguard::Severity::Warn,
        ignore: Vec::new(),
    };

    let depguard_findings = depguard::check_workspace(&repo.root, &depguard_config)?;

    let findings: Vec<Finding> = depguard_findings
        .into_iter()
        .filter(|f| f.code == "path_missing_version")
        .map(|f| {
            mk_finding(
                default_sev,
                "deps.path_missing_version",
                &f.code,
                f.message,
                f.path,
                f.line,
            )
        })
        .collect();

    Ok(CheckReport {
        id: "deps.path_missing_version".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_deps_workspace_inheritance(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = depguard::Config {
        check_wildcards: false,
        check_path_version: false,
        check_workspace_inheritance: true,
        severity: depguard::Severity::Info,
        ignore: Vec::new(),
    };

    let depguard_findings = depguard::check_workspace(&repo.root, &depguard_config)?;

    let findings: Vec<Finding> = depguard_findings
        .into_iter()
        .filter(|f| f.code == "missing_workspace_inheritance")
        .map(|f| {
            mk_finding(
                default_sev,
                "deps.workspace_inheritance",
                &f.code,
                f.message,
                f.path,
                f.line,
            )
        })
        .collect();

    Ok(CheckReport {
        id: "deps.workspace_inheritance".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_lockfile_present(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.lockfile_present";
    let mut findings = Vec::new();

    // Determine if workspace has any binary targets
    let has_any_binary = repo.workspace.members.iter().any(|m| m.has_binary_target);

    if has_any_binary && !repo.lockfile_exists {
        // Binary crate without Cargo.lock
        let binary_crates: Vec<&str> = repo
            .workspace
            .members
            .iter()
            .filter(|m| m.has_binary_target)
            .map(|m| m.name.as_str())
            .collect();

        let message = if binary_crates.len() == 1 {
            format!(
                "Cargo.lock is missing but crate '{}' has binary targets; \
                 lockfile ensures reproducible builds",
                binary_crates[0]
            )
        } else {
            format!(
                "Cargo.lock is missing but {} crates have binary targets ({}); \
                 lockfile ensures reproducible builds",
                binary_crates.len(),
                binary_crates.join(", ")
            )
        };

        findings.push(mk_finding(
            default_sev,
            CHECK_ID,
            "missing_lockfile_for_binary",
            message,
            Some("Cargo.lock".to_string()),
            None,
        ));
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

/// Check that publishable crates have required metadata.
///
/// Required fields for crates.io:
/// - description
/// - license or license-file
///
/// Recommended fields (warn):
/// - repository
/// - documentation
/// - readme
fn check_publish_ready(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "workspace.publish_ready";
    let mut findings = Vec::new();

    for m in &repo.workspace.members {
        let rel = rel_path(&repo.root, &m.manifest_path);
        let meta = &m.publish_metadata;

        // Skip crates with publish = false
        if meta.publish_disabled {
            continue;
        }

        // Required: description
        if meta.description.is_none() {
            findings.push(mk_finding(
                default_sev,
                CHECK_ID,
                "missing_description",
                format!(
                    "{}: missing required 'description' field for publishing",
                    m.name
                ),
                Some(rel.clone()),
                None,
            ));
        }

        // Required: license or license-file
        if meta.license.is_none() && meta.license_file.is_none() {
            findings.push(mk_finding(
                default_sev,
                CHECK_ID,
                "missing_license",
                format!(
                    "{}: missing required 'license' or 'license-file' field for publishing",
                    m.name
                ),
                Some(rel.clone()),
                None,
            ));
        }

        // Recommended: repository (Info level)
        if meta.repository.is_none() {
            findings.push(mk_finding(
                Severity::Info,
                CHECK_ID,
                "missing_repository",
                format!("{}: missing recommended 'repository' field", m.name),
                Some(rel.clone()),
                None,
            ));
        }

        // Recommended: documentation or homepage (Info level)
        if meta.documentation.is_none() && meta.homepage.is_none() {
            findings.push(mk_finding(
                Severity::Info,
                CHECK_ID,
                "missing_documentation",
                format!(
                    "{}: missing recommended 'documentation' or 'homepage' field",
                    m.name
                ),
                Some(rel.clone()),
                None,
            ));
        }

        // Recommended: readme (Info level)
        if meta.readme.is_none() {
            findings.push(mk_finding(
                Severity::Info,
                CHECK_ID,
                "missing_readme",
                format!("{}: missing recommended 'readme' field", m.name),
                Some(rel.clone()),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

/// Check for deprecated edition features and migration opportunities.
fn check_edition_deprecations(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "rust.edition_deprecations";
    let mut findings = Vec::new();

    // Current latest stable edition
    const LATEST_EDITION: &str = "2024";

    // Edition deprecation info
    let deprecated_editions = [(
        "2015",
        "Edition 2015 is outdated; consider migrating to 2021 or later",
    )];

    // Get workspace edition
    let workspace_edition = repo.workspace.workspace_edition.as_deref();

    // Check workspace edition
    if let Some(edition) = workspace_edition {
        // Check for deprecated editions
        for (dep_edition, msg) in &deprecated_editions {
            if edition == *dep_edition {
                findings.push(mk_finding(
                    default_sev,
                    CHECK_ID,
                    "deprecated_edition",
                    format!("workspace: {}", msg),
                    repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                    None,
                ));
            }
        }

        // Check for migration opportunity
        if edition != LATEST_EDITION && edition != "2021" {
            findings.push(mk_finding(
                Severity::Info,
                CHECK_ID,
                "edition_migration_available",
                format!(
                    "workspace: edition {} can be migrated to {} using `cargo fix --edition`",
                    edition, LATEST_EDITION
                ),
                repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                None,
            ));
        }
    }

    // Check each member for edition issues
    for m in &repo.workspace.members {
        let rel = rel_path(&repo.root, &m.manifest_path);

        // Get effective edition
        let effective_edition = if let Some(ref ed) = m.edition {
            Some(ed.as_str())
        } else if m.edition_workspace {
            workspace_edition
        } else {
            None
        };

        if let Some(edition) = effective_edition {
            for (dep_edition, msg) in &deprecated_editions {
                if edition == *dep_edition {
                    findings.push(mk_finding(
                        default_sev,
                        CHECK_ID,
                        "deprecated_edition",
                        format!("{}: {}", m.name, msg),
                        Some(rel.clone()),
                        None,
                    ));
                }
            }
        }
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

/// Check for duplicate dependency versions across workspace members.
fn check_duplicate_versions(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.duplicate_versions";
    let mut findings = Vec::new();

    // Collect all dependency versions across workspace members
    // Map: dependency name -> Map<version -> list of crates using it>
    let mut dep_versions: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    // Parse each member's Cargo.toml for dependencies
    for m in &repo.workspace.members {
        let manifest_txt = match fs::read_to_string(&m.manifest_path) {
            Ok(txt) => txt,
            Err(_) => continue,
        };
        let manifest: toml::Value = match toml::from_str(&manifest_txt) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Check all dependency sections
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = manifest.get(section).and_then(|d| d.as_table()) {
                for (dep_name, dep_value) in deps {
                    // Skip workspace inherited deps
                    if let Some(table) = dep_value.as_table()
                        && table
                            .get("workspace")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    {
                        continue;
                    }

                    // Extract version
                    let version = match dep_value {
                        toml::Value::String(v) => Some(v.clone()),
                        toml::Value::Table(t) => {
                            t.get("version").and_then(|v| v.as_str()).map(String::from)
                        }
                        _ => None,
                    };

                    if let Some(ver) = version {
                        dep_versions
                            .entry(dep_name.clone())
                            .or_default()
                            .entry(ver)
                            .or_default()
                            .push(m.name.clone());
                    }
                }
            }
        }
    }

    // Find dependencies with multiple versions
    for (dep_name, versions) in &dep_versions {
        if versions.len() > 1 {
            let version_list: Vec<String> = versions
                .iter()
                .map(|(ver, crates)| format!("{} (used by: {})", ver, crates.join(", ")))
                .collect();

            findings.push(mk_finding(
                default_sev,
                CHECK_ID,
                "duplicate_dependency_version",
                format!(
                    "dependency '{}' has multiple versions: {}",
                    dep_name,
                    version_list.join("; ")
                ),
                Some("Cargo.toml".to_string()),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

/// Check dependencies against RustSec advisory database.
///
/// This check is a placeholder that requires the `security` feature.
/// When enabled, it will use the rustsec crate to check for vulnerabilities.
fn check_security_advisory(
    repo: &RepoState,
    _config: &Config,
    _default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.security_advisory";

    // Check if Cargo.lock exists (required for security scanning)
    if !repo.lockfile_exists {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(
                "Cargo.lock not found; required for security scanning".to_string(),
            ),
        });
    }

    // Placeholder: Security check requires the rustsec crate
    // For now, return a skip status indicating the feature is not enabled
    #[cfg(not(feature = "security"))]
    {
        Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(
                "security advisory check requires the 'security' feature to be enabled".to_string(),
            ),
        })
    }

    #[cfg(feature = "security")]
    {
        check_security_advisory_impl(repo, _config, _default_sev)
    }
}

/// Implementation of security advisory check when the feature is enabled.
#[cfg(feature = "security")]
fn check_security_advisory_impl(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.security_advisory";
    let mut findings = Vec::new();

    use rustsec::{Database, Lockfile};

    // Load the advisory database
    let db = Database::fetch().context("failed to fetch RustSec advisory database")?;

    // Load Cargo.lock
    let lockfile_path = repo.root.join("Cargo.lock");
    let lockfile = Lockfile::load(&lockfile_path)
        .with_context(|| format!("failed to load {}", lockfile_path))?;

    // Check for vulnerabilities
    let vulns = db.vulnerabilities(&lockfile);

    for vuln in vulns.iter() {
        let advisory = &vuln.advisory;
        let pkg = &vuln.package;

        findings.push(mk_finding(
            default_sev,
            CHECK_ID,
            "security_vulnerability",
            format!(
                "{} {} has security advisory {}: {}",
                pkg.name, pkg.version, advisory.id, advisory.title
            ),
            Some("Cargo.lock".to_string()),
            None,
        ));
    }

    // Note: Unmaintained/yanked warnings are not available in rustsec 0.30 public API
    // These would require using cargo-audit or a different approach

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_repo::{Member, PublishMetadata, RepoState, Toolchain, WorkspaceInfo};
    use builddiag_types::{Config, MsrvSource, RelationToMsrv, Severity};
    use camino::Utf8PathBuf;

    /// Helper to create a minimal RepoState for testing
    fn mock_repo_state() -> RepoState {
        RepoState {
            root: Utf8PathBuf::from("/test/repo"),
            cargo_root: Some(Utf8PathBuf::from("/test/repo/Cargo.toml")),
            toolchain: None,
            workspace: WorkspaceInfo {
                is_workspace: true,
                members: Vec::new(),
                workspace_msrv: None,
                workspace_edition: Some("2021".to_string()),
                workspace_resolver: Some("2".to_string()),
            },
            workspace_model: None,
            tools_checksums: None,
            tools_manifest: None,
            changed_files: None,
            lockfile_exists: true,
        }
    }

    /// Helper to create a RepoState with workspace MSRV set
    fn mock_repo_with_msrv(msrv: &str) -> RepoState {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_msrv = Some(msrv.to_string());
        repo
    }

    /// Helper to create a RepoState with toolchain
    fn mock_repo_with_toolchain(channel: &str) -> RepoState {
        let mut repo = mock_repo_state();
        repo.toolchain = Some(Toolchain {
            path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
            channel: channel.to_string(),
        });
        repo
    }

    /// Helper to create a RepoState with both MSRV and toolchain
    fn mock_repo_with_msrv_and_toolchain(msrv: &str, channel: &str) -> RepoState {
        let mut repo = mock_repo_with_msrv(msrv);
        repo.toolchain = Some(Toolchain {
            path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
            channel: channel.to_string(),
        });
        repo
    }

    /// Helper to create a RepoState with workspace members
    fn mock_repo_with_members(workspace_msrv: Option<&str>, members: Vec<Member>) -> RepoState {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_msrv = workspace_msrv.map(|s| s.to_string());
        repo.workspace.members = members;
        repo
    }

    /// Helper to create a Member
    fn mock_member(name: &str, rust_version: Option<&str>, rust_version_workspace: bool) -> Member {
        Member {
            name: name.to_string(),
            manifest_path: Utf8PathBuf::from(format!("/test/repo/crates/{}/Cargo.toml", name)),
            rust_version: rust_version.map(|s| s.to_string()),
            rust_version_workspace,
            edition: Some("2021".to_string()),
            edition_workspace: true,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }
    }

    // =========================================================================
    // Task 6.1: Tests for check_msrv_defined
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn msrv_defined_passes_when_workspace_msrv_is_set() {
        // Arrange: RepoState with workspace_msrv set
        let repo = mock_repo_with_msrv("1.70.0");
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Pass, no findings
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_fails_when_workspace_msrv_is_none_and_require_defined_is_true() {
        // Arrange: RepoState without workspace_msrv, require_defined = true (default)
        let repo = mock_repo_state();
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Fail, finding has message about missing MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
        assert!(report.findings[0].message.contains("Missing"));
    }

    #[test]
    fn msrv_defined_passes_when_require_defined_is_false() {
        // Arrange: RepoState without workspace_msrv, but require_defined = false
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.msrv.require_defined = false;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Pass (no requirement to have MSRV)
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_with_any_source_passes_when_member_has_msrv() {
        // Arrange: No workspace MSRV, but a member has rust_version, source = Any
        let members = vec![mock_member("my-crate", Some("1.70.0"), false)];
        let repo = mock_repo_with_members(None, members);
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass because at least one crate has MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_with_any_source_fails_when_no_msrv_anywhere() {
        // Arrange: No workspace MSRV, no member MSRV, source = Any
        let members = vec![mock_member("my-crate", None, false)];
        let repo = mock_repo_with_members(None, members);
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail because no MSRV defined anywhere
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
    }

    #[test]
    fn msrv_defined_fails_with_invalid_msrv_workspace_source() {
        // Arrange: Workspace MSRV is set but invalid (unparseable)
        let repo = mock_repo_with_msrv("not-a-valid-version");
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail because MSRV is invalid
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "invalid_msrv_defined");
        assert!(report.findings[0].message.contains("not-a-valid-version"));
    }

    #[test]
    fn msrv_defined_fails_with_invalid_msrv_any_source() {
        // Arrange: No workspace MSRV, member has invalid MSRV, source = Any
        let members = vec![mock_member("my-crate", Some("garbage-version"), false)];
        let repo = mock_repo_with_members(None, members);
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail because MSRV is invalid
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "invalid_msrv_defined");
    }

    #[test]
    fn msrv_defined_single_crate_with_msrv() {
        // Arrange: Single crate (not a workspace) with package.rust-version set
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = false;
        repo.workspace.workspace_msrv = Some("1.75.0".to_string());
        repo.workspace.members.clear();
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, MSRV is defined
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_single_crate_without_msrv() {
        // Arrange: Single crate (not a workspace) without package.rust-version
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = false;
        repo.workspace.workspace_msrv = None;
        repo.workspace.members.clear();
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, MSRV is not defined
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
    }

    #[test]
    fn msrv_defined_includes_path_in_finding() {
        // Arrange: No MSRV, verify the finding includes path
        let repo = mock_repo_state();
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Finding includes path to Cargo.toml
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].location.is_some());
        assert!(
            report.findings[0]
                .location
                .as_ref()
                .unwrap()
                .path
                .contains("Cargo.toml")
        );
    }

    // =========================================================================
    // Task 6.2: Tests for check_msrv_consistent
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn msrv_consistent_passes_when_all_members_match_workspace_msrv() {
        // Arrange: Workspace MSRV = 1.70.0, all members inherit from workspace
        let members = vec![
            mock_member("crate-a", None, true), // inherits from workspace
            mock_member("crate-b", None, true), // inherits from workspace
        ];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, all members consistent
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_consistent_fails_when_member_has_different_msrv() {
        // Arrange: Workspace MSRV = 1.70.0, one member has different MSRV
        let members = vec![
            mock_member("crate-a", None, true),            // inherits 1.70.0
            mock_member("crate-b", Some("1.65.0"), false), // different MSRV
        ];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, member has mismatched MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "msrv_mismatch"));
    }

    #[test]
    fn msrv_consistent_skips_when_no_workspace_msrv() {
        // Arrange: No workspace MSRV to compare against
        let members = vec![mock_member("crate-a", Some("1.70.0"), false)];
        let repo = mock_repo_with_members(None, members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no workspace MSRV to compare
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no workspace"));
    }

    #[test]
    fn msrv_consistent_fails_when_member_missing_msrv_and_not_inheriting() {
        // Arrange: Workspace MSRV set, member has no MSRV and doesn't inherit
        let members = vec![mock_member("crate-a", None, false)]; // no MSRV, not inheriting
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, member missing MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_member_msrv")
        );
    }

    #[test]
    fn msrv_consistent_passes_with_allow_per_crate_override() {
        // Arrange: Member has different MSRV but allow_per_crate_override is true
        let members = vec![mock_member("crate-a", Some("1.65.0"), false)];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let mut config = Config::default();
        config.policy.msrv.allow_per_crate_override = true;

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, override allowed
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    // =========================================================================
    // Task 6.3: Tests for check_toolchain_pinning
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn toolchain_pinning_passes_when_pinned_to_specific_version() {
        // Arrange: Toolchain pinned to specific version like "1.75.0"
        let repo = mock_repo_with_toolchain("1.75.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain is pinned
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_stable() {
        // Arrange: Toolchain is "stable" (unpinned)
        let repo = mock_repo_with_toolchain("stable");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "stable" is not pinned
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_beta() {
        // Arrange: Toolchain is "beta" (unpinned)
        let repo = mock_repo_with_toolchain("beta");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "beta" is not pinned
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_nightly() {
        // Arrange: Toolchain is "nightly" (unpinned and disallowed by default)
        let repo = mock_repo_with_toolchain("nightly");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "nightly" is not pinned and disallowed
        assert_eq!(report.status, CheckStatus::Fail);
        // Should have both nightly_disallowed and unpinned_channel findings
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "nightly_disallowed" || f.code == "unpinned_channel")
        );
    }

    #[test]
    fn toolchain_pinning_fails_when_missing_toolchain_file_and_required() {
        // Arrange: No toolchain file, require_pinned = true (default)
        let repo = mock_repo_state(); // no toolchain
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, missing toolchain file
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_toolchain")
        );
    }

    #[test]
    fn toolchain_pinning_passes_when_missing_toolchain_file_and_not_required() {
        // Arrange: No toolchain file, require_pinned = false
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.toolchain.require_pinned = false;

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain not required
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_pinning_allows_nightly_when_configured() {
        // Arrange: Toolchain is "nightly" but allow_nightly = true
        let repo = mock_repo_with_toolchain("nightly");
        let mut config = Config::default();
        config.policy.toolchain.allow_nightly = true;

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Still fails for unpinned, but no nightly_disallowed finding
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "nightly_disallowed")
        );
        // But should still have unpinned_channel
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    // =========================================================================
    // Task 6.4: Tests for check_toolchain_msrv_relation
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn toolchain_msrv_relation_passes_when_toolchain_equals_msrv() {
        // Arrange: Toolchain = 1.70.0, MSRV = 1.70.0, relation = Equals (default)
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain equals MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_msrv_relation_fails_when_toolchain_less_than_msrv() {
        // Arrange: Toolchain = 1.65.0, MSRV = 1.70.0, relation = Equals
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.65.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, toolchain doesn't equal MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "toolchain_msrv_mismatch")
        );
    }

    #[test]
    fn toolchain_msrv_relation_fails_when_toolchain_greater_than_msrv_with_equals() {
        // Arrange: Toolchain = 1.75.0, MSRV = 1.70.0, relation = Equals
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.75.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, toolchain doesn't equal MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "toolchain_msrv_mismatch")
        );
    }

    #[test]
    fn toolchain_msrv_relation_passes_when_toolchain_at_least_msrv() {
        // Arrange: Toolchain = 1.75.0, MSRV = 1.70.0, relation = AtLeast
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.75.0");
        let mut config = Config::default();
        config.policy.toolchain.relation_to_msrv = RelationToMsrv::AtLeast;

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain >= MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_non_numeric_toolchain_channel() {
        // Arrange: Toolchain = "stable" (non-numeric)
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "stable");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, can't compare non-numeric channel
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("non-numeric"));
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_no_toolchain_file() {
        // Arrange: No toolchain file
        let repo = mock_repo_with_msrv("1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no toolchain file
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no toolchain"));
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_no_workspace_msrv() {
        // Arrange: Toolchain set but no MSRV
        let repo = mock_repo_with_toolchain("1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no MSRV to compare
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no workspace"));
    }

    // =========================================================================
    // Task 6.5: Tests for check_workspace_resolver
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn workspace_resolver_passes_when_resolver_is_2() {
        // Arrange: Workspace with resolver = "2"
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = Some("2".to_string());
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Pass, resolver is "2"
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn workspace_resolver_fails_when_resolver_is_missing() {
        // Arrange: Workspace with no resolver set
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = None;
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Fail, resolver not set to "2"
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.findings.iter().any(|f| f.code == "resolver_not_v2"));
    }

    #[test]
    fn workspace_resolver_fails_when_resolver_is_not_2() {
        // Arrange: Workspace with resolver = "1"
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = Some("1".to_string());
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Fail, resolver is not "2"
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.findings.iter().any(|f| f.code == "resolver_not_v2"));
    }

    #[test]
    fn workspace_resolver_skips_when_not_a_workspace() {
        // Arrange: Not a workspace (single crate)
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = false;
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Skip, not a workspace
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("not a workspace"));
    }

    // =========================================================================
    // Contract Tests: Ensure finding codes and check documentation are in sync
    // =========================================================================

    #[test]
    fn contract_every_builtin_check_has_documentation() {
        // Every check ID in BUILTIN_CHECKS must have a corresponding entry in CHECK_DOCS
        for def in super::BUILTIN_CHECKS {
            let doc = super::CHECK_DOCS.iter().find(|d| d.id == def.id);
            assert!(
                doc.is_some(),
                "Check '{}' is in BUILTIN_CHECKS but missing from CHECK_DOCS",
                def.id
            );
        }
    }

    #[test]
    fn contract_every_documented_check_is_builtin() {
        // Every check ID in CHECK_DOCS must have a corresponding entry in BUILTIN_CHECKS
        for doc in super::CHECK_DOCS {
            let def = super::BUILTIN_CHECKS.iter().find(|d| d.id == doc.id);
            assert!(
                def.is_some(),
                "Check '{}' is in CHECK_DOCS but missing from BUILTIN_CHECKS",
                doc.id
            );
        }
    }

    #[test]
    fn contract_no_duplicate_check_ids() {
        // No duplicate check IDs in BUILTIN_CHECKS
        let mut seen = std::collections::HashSet::new();
        for def in super::BUILTIN_CHECKS {
            assert!(
                seen.insert(def.id),
                "Duplicate check ID in BUILTIN_CHECKS: '{}'",
                def.id
            );
        }
    }

    #[test]
    fn contract_no_duplicate_finding_codes() {
        // No duplicate finding codes across all checks (codes should be globally unique)
        let mut seen = std::collections::HashSet::new();
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(
                    seen.insert(*code),
                    "Duplicate finding code '{}' found in check '{}'",
                    code,
                    doc.id
                );
            }
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_check_ids() {
        // explain_check should resolve every check ID
        for def in super::BUILTIN_CHECKS {
            let result = super::explain_check(def.id);
            assert!(
                result.is_some(),
                "explain_check('{}') returned None",
                def.id
            );
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_finding_codes() {
        // explain_check should resolve every finding code to its parent check
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                let result = super::explain_check(code);
                assert!(
                    result.is_some(),
                    "explain_check('{}') returned None for code from check '{}'",
                    code,
                    doc.id
                );
                // Verify it resolves to the correct check
                assert_eq!(
                    result.unwrap().id,
                    doc.id,
                    "explain_check('{}') resolved to '{}', expected '{}'",
                    code,
                    result.unwrap().id,
                    doc.id
                );
            }
        }
    }

    #[test]
    fn contract_all_checks_have_nonempty_codes() {
        // Every documented check should have at least one finding code
        for doc in super::CHECK_DOCS {
            assert!(
                !doc.codes.is_empty(),
                "Check '{}' has no finding codes defined",
                doc.id
            );
        }
    }

    #[test]
    fn contract_check_ids_follow_naming_convention() {
        // Check IDs should follow the pattern "module.check_name"
        for def in super::BUILTIN_CHECKS {
            assert!(
                def.id.contains('.'),
                "Check ID '{}' doesn't follow 'module.check_name' convention",
                def.id
            );
            let parts: Vec<&str> = def.id.split('.').collect();
            assert_eq!(
                parts.len(),
                2,
                "Check ID '{}' should have exactly one '.' separator",
                def.id
            );
            assert!(
                !parts[0].is_empty() && !parts[1].is_empty(),
                "Check ID '{}' has empty module or check name",
                def.id
            );
        }
    }

    #[test]
    fn contract_finding_codes_are_snake_case() {
        // All finding codes should be snake_case (lowercase with underscores, digits allowed)
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(
                    code.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "Finding code '{}' in check '{}' is not snake_case",
                    code,
                    doc.id
                );
                // Should not start with a digit
                assert!(
                    !code.chars().next().unwrap_or('0').is_ascii_digit(),
                    "Finding code '{}' in check '{}' starts with a digit",
                    code,
                    doc.id
                );
            }
        }
    }

    // =========================================================================
    // Tests for check_publish_ready
    // =========================================================================

    /// Helper to create a Member with publish metadata
    fn mock_member_with_publish(
        name: &str,
        description: Option<&str>,
        license: Option<&str>,
        repository: Option<&str>,
        publish_disabled: bool,
    ) -> Member {
        Member {
            name: name.to_string(),
            manifest_path: Utf8PathBuf::from(format!("/test/repo/crates/{}/Cargo.toml", name)),
            rust_version: Some("1.70.0".to_string()),
            rust_version_workspace: false,
            edition: Some("2021".to_string()),
            edition_workspace: true,
            has_binary_target: false,
            publish_metadata: PublishMetadata {
                publish_disabled,
                description: description.map(|s| s.to_string()),
                license: license.map(|s| s.to_string()),
                license_file: None,
                repository: repository.map(|s| s.to_string()),
                homepage: None,
                documentation: None,
                readme: None,
                keywords: Vec::new(),
                categories: Vec::new(),
            },
        }
    }

    #[test]
    fn publish_ready_passes_when_all_required_fields_present() {
        let members = vec![mock_member_with_publish(
            "my-crate",
            Some("A test crate"),
            Some("MIT"),
            Some("https://github.com/test/test"),
            false,
        )];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_publish_ready(&repo, &config, Severity::Warn).unwrap();

        // Should pass (no errors), but may have info-level recommendations
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "missing_description")
        );
        assert!(!report.findings.iter().any(|f| f.code == "missing_license"));
    }

    #[test]
    fn publish_ready_fails_when_missing_description() {
        let members = vec![mock_member_with_publish(
            "my-crate",
            None, // missing description
            Some("MIT"),
            Some("https://github.com/test/test"),
            false,
        )];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_publish_ready(&repo, &config, Severity::Warn).unwrap();

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_description")
        );
    }

    #[test]
    fn publish_ready_fails_when_missing_license() {
        let members = vec![mock_member_with_publish(
            "my-crate",
            Some("A test crate"),
            None, // missing license
            Some("https://github.com/test/test"),
            false,
        )];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_publish_ready(&repo, &config, Severity::Warn).unwrap();

        assert!(report.findings.iter().any(|f| f.code == "missing_license"));
    }

    #[test]
    fn publish_ready_skips_when_publish_disabled() {
        let members = vec![mock_member_with_publish(
            "my-crate", None, // missing description
            None, // missing license
            None, true, // publish = false
        )];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_publish_ready(&repo, &config, Severity::Warn).unwrap();

        // Should not have any findings since publish is disabled
        assert!(report.findings.is_empty());
    }

    #[test]
    fn publish_ready_warns_for_missing_repository() {
        let members = vec![mock_member_with_publish(
            "my-crate",
            Some("A test crate"),
            Some("MIT"),
            None, // missing repository
            false,
        )];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_publish_ready(&repo, &config, Severity::Warn).unwrap();

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_repository")
        );
        // The missing_repository should be Info level
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "missing_repository")
            .unwrap();
        assert_eq!(finding.severity, Severity::Info);
    }

    // =========================================================================
    // Tests for check_edition_deprecations
    // =========================================================================

    #[test]
    fn edition_deprecations_warns_for_edition_2015() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2015".to_string());
        let config = Config::default();

        let report = check_edition_deprecations(&repo, &config, Severity::Warn).unwrap();

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "deprecated_edition")
        );
    }

    #[test]
    fn edition_deprecations_passes_for_edition_2021() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2021".to_string());
        let config = Config::default();

        let report = check_edition_deprecations(&repo, &config, Severity::Warn).unwrap();

        // 2021 is not deprecated and not ancient enough for migration warning
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "deprecated_edition")
        );
    }

    #[test]
    fn edition_deprecations_suggests_migration_for_2018() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2018".to_string());
        let config = Config::default();

        let report = check_edition_deprecations(&repo, &config, Severity::Warn).unwrap();

        // 2018 is not deprecated but migration is available
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "deprecated_edition")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "edition_migration_available")
        );
    }

    // =========================================================================
    // Tests for check_duplicate_versions
    // =========================================================================

    #[test]
    fn duplicate_versions_passes_when_no_workspace() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_duplicate_versions(&repo, &config, Severity::Warn).unwrap();

        // No members means no duplicates
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    // =========================================================================
    // Tests for check_security_advisory
    // =========================================================================

    #[test]
    fn security_advisory_skips_when_no_lockfile() {
        let mut repo = mock_repo_state();
        repo.lockfile_exists = false;
        let config = Config::default();

        let report = check_security_advisory(&repo, &config, Severity::Error).unwrap();

        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("Cargo.lock"));
    }

    #[test]
    fn security_advisory_skips_without_feature() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_security_advisory(&repo, &config, Severity::Error).unwrap();

        // Without the 'security' feature, check should skip
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
    }
}
