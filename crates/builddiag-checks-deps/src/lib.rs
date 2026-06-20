//! Check implementations for dependency-oriented build contract validation.

use anyhow::Result;
use builddiag_domain::check_status_from_findings;
use builddiag_repo::RepoState;
use builddiag_types::{
    CheckReport, CheckStatus, Config, Finding, Location, Severity, check_skip_reasons,
};
use depguard::{Config as DepguardConfig, Severity as DepSeverity};
use std::collections::BTreeMap;
use std::fs;
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

/// Check for wildcard dependency versions.
pub fn check_deps_wildcard(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = DepguardConfig {
        check_wildcards: true,
        check_path_version: false,
        check_workspace_inheritance: false,
        severity: DepSeverity::Warn,
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
        skipped_detail: None,
    })
}

/// Check for path dependencies missing version fields.
pub fn check_deps_path_version(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = DepguardConfig {
        check_wildcards: false,
        check_path_version: true,
        check_workspace_inheritance: false,
        severity: DepSeverity::Warn,
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
        skipped_detail: None,
    })
}

/// Check for dependencies that can use workspace inheritance.
pub fn check_deps_workspace_inheritance(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let depguard_config = DepguardConfig {
        check_wildcards: false,
        check_path_version: false,
        check_workspace_inheritance: true,
        severity: DepSeverity::Info,
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
        skipped_detail: None,
    })
}

/// Ensure lockfile is present when binary targets exist.
pub fn check_lockfile_present(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.lockfile_present";
    let mut findings = Vec::new();

    let has_any_binary = repo.workspace.members.iter().any(|m| m.has_binary_target);

    if has_any_binary && !repo.lockfile_exists {
        let binary_crates: Vec<&str> = repo
            .workspace
            .members
            .iter()
            .filter(|m| m.has_binary_target)
            .map(|m| m.name.as_str())
            .collect();

        let message = if binary_crates.len() == 1 {
            format!(
                "Cargo.lock is missing but crate '{}' has binary targets; \n                 lockfile ensures reproducible builds",
                binary_crates[0]
            )
        } else {
            format!(
                "Cargo.lock is missing but {} crates have binary targets ({}); \n                 lockfile ensures reproducible builds",
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
        skipped_detail: None,
    })
}

/// Check for duplicate dependency versions across workspace members.
pub fn check_duplicate_versions(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.duplicate_versions";
    let mut findings = Vec::new();

    let mut dep_versions: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    for m in &repo.workspace.members {
        let manifest_txt = match fs::read_to_string(&m.manifest_path) {
            Ok(txt) => txt,
            Err(_) => continue,
        };
        let manifest: toml::Value = match toml::from_str(&manifest_txt) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = manifest.get(section).and_then(|d| d.as_table()) {
                for (dep_name, dep_value) in deps {
                    if let Some(table) = dep_value.as_table()
                        && table
                            .get("workspace")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    {
                        continue;
                    }

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
        skipped_detail: None,
    })
}

/// Check dependencies against RustSec advisory database.
///
/// Requires the optional `security` feature.
pub fn check_security_advisory(
    repo: &RepoState,
    _config: &Config,
    _default_sev: Severity,
) -> Result<CheckReport> {
    const CHECK_ID: &str = "deps.security_advisory";

    if !repo.lockfile_exists {
        return Ok(CheckReport {
            id: CHECK_ID.to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some(
                "Cargo.lock not found; required for security scanning".to_string(),
            ),
        });
    }

    #[cfg(not(feature = "security"))]
    return Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: CheckStatus::Skip,
        findings: Vec::new(),
        skipped_reason: Some(check_skip_reasons::FEATURE_NOT_AVAILABLE.to_string()),
        skipped_detail: Some(
            "security advisory check requires the 'security' feature to be enabled".to_string(),
        ),
    });

    #[cfg(feature = "security")]
    {
        check_security_advisory_impl(repo, _default_sev)
    }
}

/// Security advisory implementation used when the `security` feature is enabled.
#[cfg(feature = "security")]
fn check_security_advisory_impl(repo: &RepoState, default_sev: Severity) -> Result<CheckReport> {
    use anyhow::Context;
    use rustsec::{Database, Lockfile};

    const CHECK_ID: &str = "deps.security_advisory";
    let mut findings = Vec::new();

    let db = Database::fetch().context("failed to fetch RustSec advisory database")?;

    let lockfile_path = repo.root.join("Cargo.lock");
    let lockfile = Lockfile::load(&lockfile_path)
        .with_context(|| format!("failed to load {}", lockfile_path))?;

    let vulns = db.vulnerabilities(&lockfile);

    for vuln in vulns.iter() {
        let advisory = &vuln.advisory;
        let pkg = &vuln.package;

        findings.push(Finding {
            check_id: CHECK_ID.to_string(),
            code: "security_vulnerability".to_string(),
            severity: default_sev,
            message: format!(
                "{} {} has security advisory {}: {}",
                pkg.name, pkg.version, advisory.id, advisory.title
            ),
            location: Some(Location {
                path: "Cargo.lock".to_string(),
                line: None,
                col: None,
            }),
        });
    }

    Ok(CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
        skipped_detail: None,
    })
}
