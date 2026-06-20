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

use anyhow::{Result, anyhow};
pub use builddiag_checks_catalog::{
    BUILTIN_CHECKS, CHECK_DOCS, CheckDef, CheckDocumentation, explain_check,
};
use builddiag_domain::{check_status_from_findings, parse_rust_version};
use builddiag_paths::to_repo_relative;
use builddiag_repo::{RepoState, maybe_parse_numeric_version};
use builddiag_types::{
    CheckConfig, CheckReport, CheckStatus, Config, Finding, Location, RelationToMsrv, Severity,
    check_skip_reasons, effective_check_config,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::collections::{BTreeSet, HashSet};

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
                    skip_reason: Some(check_skip_reasons::DISABLED_BY_CONFIG.to_string()),
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
                    skip_reason: Some(check_skip_reasons::DIFF_AWARE_NO_MATCH.to_string()),
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
            skipped_detail: None,
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

    if report.skipped_reason.is_none() {
        report.status = check_status_from_findings(&report.findings);
    }
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
            let any_msrv = msrv.clone().or_else(|| {
                repo.workspace
                    .members
                    .iter()
                    .find_map(|m| m.rust_version.clone())
            });
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
            }

            if any_msrv.is_none() && config.policy.msrv.require_defined {
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
        skipped_detail: None,
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
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no workspace/package MSRV to compare".to_string()),
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
                skipped_detail: None,
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
        skipped_detail: None,
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
            skipped_detail: None,
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

    config.policy.toolchain.require_pinned.then(|| {
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
        } else {
            match maybe_parse_numeric_version(channel) {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    findings.push(mk_finding(
                        default_sev,
                        "rust.toolchain_pinning",
                        "invalid_toolchain_version",
                        format!(
                            "Toolchain channel '{channel}' is not a valid numeric Rust version"
                        ),
                        Some(rel_path(&repo.root, &tc.path)),
                        None,
                    ));
                }
            }
        }
    });

    Ok(CheckReport {
        id: "rust.toolchain_pinning".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
        skipped_detail: None,
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
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no toolchain file".to_string()),
        });
    };
    let Some(msrv_raw) = &repo.workspace.workspace_msrv else {
        return Ok(CheckReport {
            id: "rust.toolchain_msrv_relation".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no workspace/package MSRV".to_string()),
        });
    };

    let toolchain_ver = match maybe_parse_numeric_version(&tc.channel)? {
        Some(v) => v,
        None => {
            return Ok(CheckReport {
                id: "rust.toolchain_msrv_relation".to_string(),
                status: CheckStatus::Skip,
                findings: Vec::new(),
                skipped_reason: Some(check_skip_reasons::NOT_APPLICABLE.to_string()),
                skipped_detail: Some("non-numeric toolchain channel".to_string()),
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
        skipped_detail: None,
    })
}

fn check_checksums_file_exists(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_checksums::check_checksums_file_exists(repo, config, default_sev)
}

fn check_checksums_format(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_checksums::check_checksums_format(repo, _config, default_sev)
}

fn check_checksums_coverage(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_checksums::check_checksums_coverage(repo, config, default_sev)
}

fn check_checksums_verify_local(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_checksums::check_checksums_verify_local(repo, config, default_sev)
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
            skipped_reason: Some(check_skip_reasons::NOT_APPLICABLE.to_string()),
            skipped_detail: Some("not a workspace".to_string()),
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
        skipped_detail: None,
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
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no workspace edition to compare".to_string()),
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
            skipped_detail: None,
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
        skipped_detail: None,
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
            skipped_reason: Some(check_skip_reasons::NOT_APPLICABLE.to_string()),
            skipped_detail: Some("not a workspace".to_string()),
        });
    }

    if !config.policy.member_ordering.require_sorted {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some(check_skip_reasons::DISABLED_BY_POLICY.to_string()),
            skipped_detail: Some("sorting not required by policy".to_string()),
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
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("workspace model not available".to_string()),
        });
    };

    let patterns = &model.member_patterns;
    if patterns.is_empty() {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Pass,
            findings,
            skipped_reason: None,
            skipped_detail: None,
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
        skipped_detail: None,
    })
}

fn rel_path(root: &camino::Utf8Path, p: &camino::Utf8Path) -> String {
    to_repo_relative(root, p)
}

fn check_deps_wildcard(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_deps_wildcard(repo, _config, default_sev)
}

fn check_deps_path_version(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_deps_path_version(repo, _config, default_sev)
}

fn check_deps_workspace_inheritance(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_deps_workspace_inheritance(repo, _config, default_sev)
}

fn check_lockfile_present(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_lockfile_present(repo, _config, default_sev)
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
        skipped_detail: None,
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
        skipped_detail: None,
    })
}

/// Check for duplicate dependency versions across workspace members.
fn check_duplicate_versions(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_duplicate_versions(repo, _config, default_sev)
}

/// Check dependencies against RustSec advisory database.
fn check_security_advisory(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    builddiag_checks_deps::check_security_advisory(repo, config, default_sev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_repo::{
        ChecksumEntry, Member, PublishMetadata, RepoState, Toolchain, ToolsChecksums, WorkspaceInfo,
    };
    use builddiag_types::{CheckConfig, Config, MsrvSource, RelationToMsrv, Severity};
    use camino::Utf8PathBuf;
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, BTreeSet};
    use tempfile::TempDir;

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

    fn repo_with_root(root: Utf8PathBuf) -> RepoState {
        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.cargo_root = Some(root.join("Cargo.toml"));
        repo.workspace.is_workspace = false;
        repo.workspace.members.clear();
        repo
    }

    fn repo_with_temp_root() -> (TempDir, RepoState) {
        let temp = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf())
            .expect("path should be valid UTF-8");
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write Cargo.toml");

        let repo = repo_with_root(root);
        (temp, repo)
    }

    #[test]
    fn execute_check_covers_all_builtin_checks() {
        let (_temp, mut repo) = repo_with_temp_root();
        // Skip the security advisory check: with `--all-features` (security)
        // it tries to load a real Cargo.lock from the test root and fetch the
        // RustSec database online. Setting lockfile_exists=false makes the
        // check short-circuit to Skip regardless of feature state.
        repo.lockfile_exists = false;
        let config = Config::default();

        for def in BUILTIN_CHECKS {
            let task = CheckTask {
                def,
                effective_severity: Severity::Warn,
                skip_reason: None,
            };
            let report = execute_check(&task, &repo, &config).unwrap();
            assert_eq!(report.id, def.id);
        }
    }

    #[test]
    fn run_selected_checks_propagates_errors() {
        let (_temp, mut repo) = repo_with_temp_root();
        let config = Config {
            profile: builddiag_types::Profile::Strict,
            policy: builddiag_types::Policy {
                checksums: builddiag_types::ChecksumsPolicy {
                    verify_local_files: true,
                    ..builddiag_types::ChecksumsPolicy::default()
                },
                ..builddiag_types::Policy::default()
            },
            ..Config::default()
        };

        let tools_dir = repo.root.join("scripts").join("tools");
        std::fs::create_dir_all(&tools_dir).expect("create tools dir");

        let checksums_path = repo.root.join(&config.paths.tools_checksums);
        repo.tools_checksums = Some(ToolsChecksums {
            path: checksums_path,
            entries: vec![ChecksumEntry {
                line: 1,
                hash: "deadbeef".to_string(),
                path: "scripts/tools".to_string(),
            }],
        });

        let result = run_selected_checks(&repo, &config, true);
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("read"));
    }

    #[test]
    fn deps_checks_error_when_cargo_toml_missing() {
        let temp = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf())
            .expect("path should be valid UTF-8");
        let repo = repo_with_root(root);
        let config = Config::default();

        assert!(check_deps_wildcard(&repo, &config, Severity::Warn).is_err());
        assert!(check_deps_path_version(&repo, &config, Severity::Warn).is_err());
        assert!(check_deps_workspace_inheritance(&repo, &config, Severity::Warn).is_err());
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
        config.policy.msrv.require_defined = true;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail because no MSRV defined anywhere
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
    }

    #[test]
    fn msrv_defined_with_any_source_requires_defined_when_missing_everywhere() {
        let repo = mock_repo_with_members(None, Vec::new());
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;
        config.policy.msrv.require_defined = true;

        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
        assert!(report.skipped_detail.unwrap().contains("no workspace"));
    }

    #[test]
    fn msrv_consistent_fails_with_invalid_workspace_msrv() {
        let repo = mock_repo_with_members(Some("not-a-version"), vec![]);
        let config = Config::default();

        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "invalid_msrv"));
    }

    #[test]
    fn msrv_consistent_fails_with_invalid_member_msrv() {
        let members = vec![mock_member("crate-a", Some("bogus"), false)];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "invalid_member_msrv")
        );
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
        let mut config = Config::default();
        config.policy.toolchain.require_pinned = true;

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
                .any(|f| f.code == "nightly_disallowed")
        );
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::NOT_APPLICABLE)
        );
        assert!(report.skipped_detail.unwrap().contains("non-numeric"));
    }

    #[test]
    fn toolchain_msrv_relation_errors_on_invalid_toolchain_version() {
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.bad");
        let config = Config::default();

        let err = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap_err();
        let err_msg = format!("{:#}", err);
        assert!(err_msg.contains("invalid"));
    }

    #[test]
    fn toolchain_msrv_relation_errors_on_invalid_msrv() {
        let repo = mock_repo_with_msrv_and_toolchain("nope", "1.70.0");
        let config = Config::default();

        let err = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap_err();
        let err_msg = format!("{:#}", err);
        assert!(err_msg.contains("invalid MSRV"));
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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
        assert!(report.skipped_detail.unwrap().contains("no toolchain"));
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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
        assert!(report.skipped_detail.unwrap().contains("no workspace"));
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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::NOT_APPLICABLE)
        );
        assert!(report.skipped_detail.unwrap().contains("not a workspace"));
    }

    // =========================================================================
    // Contract Tests: Ensure finding codes and check documentation are in sync
    // =========================================================================

    #[test]
    fn contract_every_builtin_check_has_documentation() {
        // Every check ID in BUILTIN_CHECKS must have a corresponding entry in CHECK_DOCS
        for def in super::BUILTIN_CHECKS {
            let doc = super::CHECK_DOCS.iter().find(|d| d.id == def.id);
            assert!(doc.is_some());
        }
    }

    #[test]
    fn contract_every_documented_check_is_builtin() {
        // Every check ID in CHECK_DOCS must have a corresponding entry in BUILTIN_CHECKS
        for doc in super::CHECK_DOCS {
            let def = super::BUILTIN_CHECKS.iter().find(|d| d.id == doc.id);
            assert!(def.is_some());
        }
    }

    #[test]
    fn contract_no_duplicate_check_ids() {
        // No duplicate check IDs in BUILTIN_CHECKS
        let mut seen = std::collections::HashSet::new();
        for def in super::BUILTIN_CHECKS {
            assert!(seen.insert(def.id));
        }
    }

    #[test]
    fn contract_no_duplicate_finding_codes() {
        // No duplicate finding codes across all checks (codes should be globally unique)
        let mut seen = std::collections::HashSet::new();
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(seen.insert(*code));
            }
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_check_ids() {
        // explain_check should resolve every check ID
        for def in super::BUILTIN_CHECKS {
            let result = super::explain_check(def.id);
            assert!(result.is_some());
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_finding_codes() {
        // explain_check should resolve every finding code to its parent check
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                let result = super::explain_check(code);
                assert!(result.is_some());
                // Verify it resolves to the correct check
                assert_eq!(result.unwrap().id, doc.id);
            }
        }
    }

    #[test]
    fn contract_all_checks_have_nonempty_codes() {
        // Every documented check should have at least one finding code
        for doc in super::CHECK_DOCS {
            assert!(!doc.codes.is_empty());
        }
    }

    #[test]
    fn contract_check_ids_follow_naming_convention() {
        // Check IDs should follow the pattern "module.check_name"
        for def in super::BUILTIN_CHECKS {
            assert!(def.id.contains('.'));
            let parts: Vec<&str> = def.id.split('.').collect();
            assert_eq!(parts.len(), 2);
            assert!(!parts[0].is_empty() && !parts[1].is_empty());
        }
    }

    #[test]
    fn contract_finding_codes_are_snake_case() {
        // All finding codes should be snake_case (lowercase with underscores, digits allowed)
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(
                    code.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                );
                // Should not start with a digit
                assert!(!code.chars().next().unwrap_or('0').is_ascii_digit());
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

    #[test]
    fn publish_ready_warns_for_missing_docs_and_readme() {
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

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_documentation")
        );
        assert!(report.findings.iter().any(|f| f.code == "missing_readme"));
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
        assert!(report.findings.is_empty());
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

    #[test]
    fn edition_deprecations_handles_member_without_edition() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = None;
        repo.workspace.members = vec![Member {
            name: "no-edition".to_string(),
            manifest_path: Utf8PathBuf::from("/test/repo/no-edition/Cargo.toml"),
            rust_version: None,
            rust_version_workspace: false,
            edition: None,
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }];

        let report = check_edition_deprecations(&repo, &Config::default(), Severity::Info).unwrap();
        assert!(report.findings.is_empty());
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

    #[test]
    fn duplicate_versions_skips_unreadable_manifests() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        let invalid_dir = root.join("invalid");
        std::fs::create_dir_all(&invalid_dir).unwrap();
        std::fs::write(invalid_dir.join("Cargo.toml"), "not = [valid").unwrap();

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.workspace.members = vec![
            Member {
                name: "missing".to_string(),
                manifest_path: root.join("missing/Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: None,
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
            Member {
                name: "invalid".to_string(),
                manifest_path: invalid_dir.join("Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: None,
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
        ];

        let report = check_duplicate_versions(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn duplicate_versions_ignores_workspace_inherited_deps() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let member_dir = root.join("crates/a");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("Cargo.toml"),
            r#"
[package]
name = "a"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
anyhow = { version = "1.0" }
weird = 1
"#,
        )
        .unwrap();

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.workspace.members = vec![Member {
            name: "a".to_string(),
            manifest_path: member_dir.join("Cargo.toml"),
            rust_version: None,
            rust_version_workspace: false,
            edition: Some("2021".to_string()),
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }];

        let report = check_duplicate_versions(&repo, &Config::default(), Severity::Warn).unwrap();
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
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
        assert!(report.skipped_detail.unwrap().contains("Cargo.lock"));
    }

    #[cfg(not(feature = "security"))]
    #[test]
    fn security_advisory_skips_without_feature() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_security_advisory(&repo, &config, Severity::Error).unwrap();

        // Without the 'security' feature, check should skip with FEATURE_NOT_AVAILABLE.
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::FEATURE_NOT_AVAILABLE)
        );
    }

    // =========================================================================
    // Additional coverage for task preparation and diff-aware handling
    // =========================================================================

    #[test]
    fn prepare_check_tasks_respects_disabled_and_diff_aware() {
        let config = Config {
            checks: vec![CheckConfig {
                id: "rust.msrv_defined".to_string(),
                severity: Severity::Warn,
                enabled: false,
                triggers: Vec::new(),
            }],
            ..Default::default()
        };

        let changed: BTreeSet<String> = ["docs/README.md".to_string()].into_iter().collect();
        let tasks = prepare_check_tasks(&config, Some(&changed), false);

        let msrv = tasks
            .iter()
            .find(|t| t.def.id == "rust.msrv_defined")
            .unwrap();
        assert_eq!(
            msrv.skip_reason.as_deref(),
            Some(check_skip_reasons::DISABLED_BY_CONFIG)
        );

        let resolver = tasks
            .iter()
            .find(|t| t.def.id == "workspace.resolver_v2")
            .unwrap();
        assert_eq!(
            resolver.skip_reason.as_deref(),
            Some(check_skip_reasons::DIFF_AWARE_NO_MATCH)
        );

        let tasks_allow_all = prepare_check_tasks(&config, Some(&changed), true);
        let resolver_allow_all = tasks_allow_all
            .iter()
            .find(|t| t.def.id == "workspace.resolver_v2")
            .unwrap();
        assert!(resolver_allow_all.skip_reason.is_none());
    }

    #[test]
    fn execute_check_returns_error_for_unknown_check_id() {
        let def = CheckDef {
            id: "unknown.check",
            default_severity: Severity::Warn,
            default_triggers: &[],
        };
        let task = CheckTask {
            def: &def,
            effective_severity: Severity::Warn,
            skip_reason: None,
        };
        let repo = mock_repo_state();
        let config = Config::default();

        let err = execute_check(&task, &repo, &config).unwrap_err();
        assert!(err.to_string().contains("unknown check id"));
    }

    #[test]
    fn effective_triggers_uses_override_when_present() {
        let def = BUILTIN_CHECKS
            .iter()
            .find(|d| d.id == "rust.msrv_defined")
            .unwrap();
        let ov = CheckConfig {
            id: def.id.to_string(),
            severity: Severity::Warn,
            enabled: true,
            triggers: vec!["custom.trigger".to_string()],
        };

        let triggers = effective_triggers(def, Some(&ov));
        assert_eq!(triggers, vec!["custom.trigger".to_string()]);
    }

    #[test]
    fn should_run_handles_invalid_globs_and_matches() {
        let changed: BTreeSet<String> = ["src/lib.rs".to_string()].into_iter().collect();
        assert!(should_run(Some(&changed), &[]));
        assert!(should_run(Some(&changed), &["[".to_string()])); // invalid glob -> fail open
        assert!(should_run(Some(&changed), &["src/**".to_string()]));
        assert!(!should_run(Some(&changed), &["Cargo.toml".to_string()]));
    }

    // =========================================================================
    // Toolchain policy coverage
    // =========================================================================

    #[test]
    fn toolchain_pinning_flags_nightly_and_unpinned() {
        let repo = mock_repo_with_toolchain("nightly");
        let config = Config::default();

        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"nightly_disallowed"));
        assert!(codes.contains(&"unpinned_channel"));
    }

    #[test]
    fn toolchain_pinning_invalid_version_reports_finding() {
        let repo = mock_repo_with_toolchain("1.x");
        let config = Config::default();

        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "invalid_toolchain_version")
        );
    }

    #[test]
    fn toolchain_msrv_relation_mismatch_equals() {
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.71.0");
        let mut config = Config::default();
        config.policy.toolchain.relation_to_msrv = RelationToMsrv::Equals;

        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "toolchain_msrv_mismatch")
        );
    }

    #[test]
    fn toolchain_msrv_relation_at_least_passes() {
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.71.0");
        let mut config = Config::default();
        config.policy.toolchain.relation_to_msrv = RelationToMsrv::AtLeast;

        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Pass);
    }

    // =========================================================================
    // Checksums coverage
    // =========================================================================

    #[test]
    fn checksums_file_exists_reports_missing_when_required() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_checksums_file_exists(&repo, &config, Severity::Error).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_checksums")
        );
    }

    #[test]
    fn checksums_format_skips_without_checksums_file() {
        let repo = mock_repo_state();

        let report = check_checksums_format(&repo, &Config::default(), Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
    }

    #[test]
    fn checksums_format_reports_invalid_missing_duplicate() {
        let mut repo = mock_repo_state();
        let valid_hash = "a".repeat(64);
        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: Utf8PathBuf::from("/test/repo/scripts/tools.sha256"),
            entries: vec![
                builddiag_repo::ChecksumEntry {
                    line: 1,
                    hash: "abc".to_string(),
                    path: "tool.bin".to_string(),
                },
                builddiag_repo::ChecksumEntry {
                    line: 2,
                    hash: valid_hash.clone(),
                    path: "".to_string(),
                },
                builddiag_repo::ChecksumEntry {
                    line: 3,
                    hash: valid_hash,
                    path: "tool.bin".to_string(),
                },
            ],
        });

        let report = check_checksums_format(&repo, &Config::default(), Severity::Error).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"invalid_hash"));
        assert!(codes.contains(&"missing_path"));
        assert!(codes.contains(&"duplicate_path"));
    }

    #[test]
    fn checksums_coverage_skips_when_policy_disabled() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_checksums_coverage(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::DISABLED_BY_POLICY)
        );
    }

    #[test]
    fn checksums_coverage_skips_without_checksums_file() {
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.checksums.require_coverage = true;

        let report = check_checksums_coverage(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
    }

    #[test]
    fn checksums_coverage_skips_without_tools_manifest() {
        let mut repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.checksums.require_coverage = true;
        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: Utf8PathBuf::from("/test/repo/scripts/tools.sha256"),
            entries: Vec::new(),
        });

        let report = check_checksums_coverage(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
    }

    #[test]
    fn checksums_coverage_reports_missing_and_unexpected() {
        let mut repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.checksums.require_coverage = true;

        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: Utf8PathBuf::from("/test/repo/scripts/tools.sha256"),
            entries: vec![
                builddiag_repo::ChecksumEntry {
                    line: 1,
                    hash: "a".repeat(64),
                    path: "tool_a.bin".to_string(),
                },
                builddiag_repo::ChecksumEntry {
                    line: 2,
                    hash: "b".repeat(64),
                    path: "extra.bin".to_string(),
                },
            ],
        });

        repo.tools_manifest = Some((
            Utf8PathBuf::from("/test/repo/scripts/tools.toml"),
            builddiag_repo::ToolsManifest {
                tool: vec![builddiag_repo::ToolDecl {
                    name: "tools".to_string(),
                    version: None,
                    files: vec!["tool_a.bin".to_string(), "tool_b.bin".to_string()],
                }],
            },
        ));

        let report = check_checksums_coverage(&repo, &config, Severity::Error).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"missing_checksum"));
        assert!(codes.contains(&"unexpected_checksum"));
    }

    #[test]
    fn checksums_verify_local_skips_when_policy_disabled() {
        let repo = mock_repo_state();
        let config = Config::default();

        let report = check_checksums_verify_local(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::DISABLED_BY_POLICY)
        );
    }

    #[test]
    fn checksums_verify_local_skips_without_checksums_file() {
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.checksums.verify_local_files = true;

        let report = check_checksums_verify_local(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
    }

    #[test]
    fn checksums_verify_local_reports_missing_and_mismatch() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let file_path = root.join("tool.bin");
        std::fs::write(&file_path, b"data").unwrap();

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: root.join("scripts/tools.sha256"),
            entries: vec![
                builddiag_repo::ChecksumEntry {
                    line: 1,
                    hash: "0".repeat(64),
                    path: "missing.bin".to_string(),
                },
                builddiag_repo::ChecksumEntry {
                    line: 2,
                    hash: "0".repeat(64),
                    path: "tool.bin".to_string(),
                },
            ],
        });

        let mut config = Config::default();
        config.policy.checksums.verify_local_files = true;

        let report = check_checksums_verify_local(&repo, &config, Severity::Error).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"missing_tool_file"));
        assert!(codes.contains(&"hash_mismatch"));
    }

    #[test]
    fn checksums_verify_local_passes_with_matching_hash() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let file_path = root.join("tool.bin");
        std::fs::write(&file_path, b"data").unwrap();

        let mut hasher = Sha256::new();
        hasher.update(b"data");
        let hash = format!("{:x}", hasher.finalize());

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: root.join("scripts/tools.sha256"),
            entries: vec![builddiag_repo::ChecksumEntry {
                line: 1,
                hash,
                path: "tool.bin".to_string(),
            }],
        });

        let mut config = Config::default();
        config.policy.checksums.verify_local_files = true;

        let report = check_checksums_verify_local(&repo, &config, Severity::Error).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn checksums_verify_local_errors_on_directory_path() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let dir_path = root.join("tool_dir");
        std::fs::create_dir_all(&dir_path).unwrap();

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.tools_checksums = Some(builddiag_repo::ToolsChecksums {
            path: root.join("scripts/tools.sha256"),
            entries: vec![builddiag_repo::ChecksumEntry {
                line: 1,
                hash: "0".repeat(64),
                path: "tool_dir".to_string(),
            }],
        });

        let mut config = Config::default();
        config.policy.checksums.verify_local_files = true;

        let err = check_checksums_verify_local(&repo, &config, Severity::Error).unwrap_err();
        let err_msg = format!("{:#}", err);
        assert!(err_msg.contains("read"));
    }

    // =========================================================================
    // Workspace resolver and edition consistency coverage
    // =========================================================================

    #[test]
    fn workspace_resolver_skips_for_non_workspace() {
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = false;
        let report = check_workspace_resolver(&repo, &Config::default(), Severity::Warn).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::NOT_APPLICABLE)
        );
    }

    #[test]
    fn workspace_resolver_reports_non_v2() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_resolver = Some("1".to_string());
        let report = check_workspace_resolver(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(report.findings.iter().any(|f| f.code == "resolver_not_v2"));
    }

    #[test]
    fn edition_consistent_skips_without_workspace_edition() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = None;
        let report = check_edition_consistent(&repo, &Config::default(), Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
    }

    #[test]
    fn edition_consistent_reports_invalid_and_missing_members() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2099".to_string());
        let report = check_edition_consistent(&repo, &Config::default(), Severity::Error).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "invalid_workspace_edition")
        );

        repo.workspace.workspace_edition = Some("2021".to_string());
        repo.workspace.members = vec![
            Member {
                name: "no-edition".to_string(),
                manifest_path: Utf8PathBuf::from("/test/repo/no-edition/Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: None,
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
            Member {
                name: "bad-edition".to_string(),
                manifest_path: Utf8PathBuf::from("/test/repo/bad-edition/Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: Some("2099".to_string()),
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
            Member {
                name: "mismatch".to_string(),
                manifest_path: Utf8PathBuf::from("/test/repo/mismatch/Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: Some("2018".to_string()),
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
        ];

        let report = check_edition_consistent(&repo, &Config::default(), Severity::Error).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"missing_member_edition"));
        assert!(codes.contains(&"invalid_member_edition"));
        assert!(codes.contains(&"edition_mismatch"));
    }

    #[test]
    fn edition_consistent_allows_override() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2021".to_string());
        repo.workspace.members = vec![Member {
            name: "override".to_string(),
            manifest_path: Utf8PathBuf::from("/test/repo/override/Cargo.toml"),
            rust_version: None,
            rust_version_workspace: false,
            edition: Some("2018".to_string()),
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }];

        let mut config = Config::default();
        config.policy.edition.allow_per_crate_override = true;

        let report = check_edition_consistent(&repo, &config, Severity::Error).unwrap();
        assert_eq!(report.status, CheckStatus::Pass);
    }

    // =========================================================================
    // Member ordering coverage
    // =========================================================================

    #[test]
    fn member_ordering_reports_unsorted_patterns() {
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace_model = Some(builddiag_repo::WorkspaceModel {
            root_manifest: builddiag_repo::ParsedManifest {
                value: toml::Value::Table(toml::map::Map::new()),
                package_name: None,
                rust_version: None,
                rust_version_workspace: false,
                edition: None,
                edition_workspace: false,
            },
            member_manifests: BTreeMap::new(),
            is_virtual: false,
            workspace_msrv: None,
            workspace_edition: None,
            workspace_resolver: None,
            member_patterns: vec!["b".to_string(), "a".to_string()],
            exclude_patterns: Vec::new(),
        });

        let report = check_member_ordering(&repo, &Config::default(), Severity::Info).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "members_not_sorted")
        );
    }

    #[test]
    fn member_ordering_skips_when_policy_disabled() {
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        let mut config = Config::default();
        config.policy.member_ordering.require_sorted = false;

        let report = check_member_ordering(&repo, &config, Severity::Info).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::DISABLED_BY_POLICY)
        );
    }

    #[test]
    fn member_ordering_skips_without_workspace_model() {
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace_model = None;
        repo.cargo_root = None;

        let report = check_member_ordering(&repo, &Config::default(), Severity::Info).unwrap();
        assert_eq!(report.status, CheckStatus::Skip);
        assert_eq!(
            report.skipped_reason.as_deref(),
            Some(check_skip_reasons::MISSING_PREREQUISITE)
        );
    }

    // =========================================================================
    // Dependency checks (depguard integration)
    // =========================================================================

    #[test]
    fn lockfile_present_reports_missing_for_single_binary() {
        let mut repo = mock_repo_state();
        repo.lockfile_exists = false;
        let mut member = mock_member("app", None, false);
        member.has_binary_target = true;
        repo.workspace.members = vec![member];

        let report = check_lockfile_present(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_lockfile_for_binary")
        );
    }

    #[test]
    fn lockfile_present_reports_missing_for_multiple_binaries() {
        let mut repo = mock_repo_state();
        repo.lockfile_exists = false;
        let mut member_a = mock_member("app-a", None, false);
        member_a.has_binary_target = true;
        let mut member_b = mock_member("app-b", None, false);
        member_b.has_binary_target = true;
        repo.workspace.members = vec![member_a, member_b];

        let report = check_lockfile_present(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_lockfile_for_binary")
        );
    }

    #[test]
    fn deps_checks_use_depguard_findings() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "dep-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "*"
path_dep = { path = "../path_dep" }
"#,
        )
        .unwrap();

        let repo = repo_with_root(root);
        let report = check_deps_wildcard(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(report.findings.iter().any(|f| f.code == "wildcard_version"));

        let report = check_deps_path_version(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "path_missing_version")
        );
    }

    #[test]
    fn deps_workspace_inheritance_reports_suggestions() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = []

[workspace.dependencies]
serde = "1.0"

[package]
name = "dep-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#,
        )
        .unwrap();

        let repo = repo_with_root(root);
        let report =
            check_deps_workspace_inheritance(&repo, &Config::default(), Severity::Info).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_workspace_inheritance")
        );
    }

    // =========================================================================
    // Publish readiness and edition deprecations
    // =========================================================================

    #[test]
    fn publish_ready_reports_missing_fields() {
        let mut repo = mock_repo_state();
        repo.workspace.members = vec![Member {
            name: "publish-me".to_string(),
            manifest_path: Utf8PathBuf::from("/test/repo/publish-me/Cargo.toml"),
            rust_version: None,
            rust_version_workspace: false,
            edition: Some("2021".to_string()),
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }];

        let report = check_publish_ready(&repo, &Config::default(), Severity::Warn).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"missing_description"));
        assert!(codes.contains(&"missing_license"));
        assert!(codes.contains(&"missing_repository"));
        assert!(codes.contains(&"missing_documentation"));
        assert!(codes.contains(&"missing_readme"));
    }

    #[test]
    fn edition_deprecations_reports_deprecated_and_migration() {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_edition = Some("2015".to_string());
        repo.workspace.members = vec![Member {
            name: "legacy".to_string(),
            manifest_path: Utf8PathBuf::from("/test/repo/legacy/Cargo.toml"),
            rust_version: None,
            rust_version_workspace: false,
            edition: Some("2015".to_string()),
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        }];

        let report = check_edition_deprecations(&repo, &Config::default(), Severity::Info).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        assert!(codes.contains(&"deprecated_edition"));
        assert!(codes.contains(&"edition_migration_available"));
    }

    // =========================================================================
    // Duplicate dependency version coverage
    // =========================================================================

    #[test]
    fn duplicate_versions_reports_multiple_versions() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        let member_a = root.join("crates/a");
        let member_b = root.join("crates/b");
        std::fs::create_dir_all(&member_a).unwrap();
        std::fs::create_dir_all(&member_b).unwrap();

        std::fs::write(
            member_a.join("Cargo.toml"),
            r#"
[package]
name = "a"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            member_b.join("Cargo.toml"),
            r#"
[package]
name = "b"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.1"
"#,
        )
        .unwrap();

        let mut repo = mock_repo_state();
        repo.root = root.clone();
        repo.workspace.members = vec![
            Member {
                name: "a".to_string(),
                manifest_path: member_a.join("Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: Some("2021".to_string()),
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
            Member {
                name: "b".to_string(),
                manifest_path: member_b.join("Cargo.toml"),
                rust_version: None,
                rust_version_workspace: false,
                edition: Some("2021".to_string()),
                edition_workspace: false,
                has_binary_target: false,
                publish_metadata: PublishMetadata::default(),
            },
        ];

        let report = check_duplicate_versions(&repo, &Config::default(), Severity::Warn).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "duplicate_dependency_version")
        );
    }
}
