//! Check implementations for checksums-related build contract validation.

use anyhow::{Context, Result};
use builddiag_domain::check_status_from_findings;
use builddiag_repo::RepoState;
use builddiag_types::{
    CheckReport, CheckStatus, Config, Finding, Location, Severity, check_skip_reasons,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
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

/// Ensures a checksums file exists when policy requires it.
pub fn check_checksums_file_exists(
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
        skipped_detail: None,
    })
}

/// Validates checksums file formatting and uniqueness.
pub fn check_checksums_format(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_format".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no checksums file".to_string()),
        });
    };

    let mut findings = Vec::new();
    let mut seen_paths = HashSet::new();

    for e in &cks.entries {
        let rel = builddiag_paths::to_repo_relative(&repo.root, &cks.path);

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
        skipped_detail: None,
    })
}

/// Ensures checksum coverage is complete for manifest-tracked tool files.
pub fn check_checksums_coverage(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    if !config.policy.checksums.require_coverage {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::DISABLED_BY_POLICY.to_string()),
            skipped_detail: Some("coverage not required by policy".to_string()),
        });
    }

    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no checksums file".to_string()),
        });
    };

    let Some((_manifest_path, manifest)) = &repo.tools_manifest else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no tools manifest".to_string()),
        });
    };

    let have: BTreeSet<String> = cks.entries.iter().map(|e| e.path.clone()).collect();
    let mut expected = BTreeSet::new();
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
        skipped_detail: None,
    })
}

/// Verifies local tool files match recorded checksums.
pub fn check_checksums_verify_local(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    if !config.policy.checksums.verify_local_files {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::DISABLED_BY_POLICY.to_string()),
            skipped_detail: Some("local verification not enabled".to_string()),
        });
    }

    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
            skipped_detail: Some("no checksums file".to_string()),
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
        skipped_detail: None,
    })
}
