//! `rust.msrv_consistent` check — orchestrates the four SRP submodules
//! (workspace MSRV parsing, per-member resolution, mismatch detection).

use anyhow::Result;
use builddiag_domain::check_status_from_findings;
use builddiag_repo::RepoState;
use builddiag_types::{CheckReport, CheckStatus, Config, Finding, Severity, check_skip_reasons};

mod member_msrv;
mod mismatch;
mod workspace_msrv;

pub(crate) const CHECK_ID: &str = "rust.msrv_consistent";

/// Validate that all workspace members have a consistent MSRV.
pub(crate) fn check_msrv_consistent(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
    let Some(workspace_msrv_raw) = repo.workspace.workspace_msrv.clone() else {
        return Ok(skip_report("no workspace/package MSRV to compare"));
    };

    let workspace_msrv =
        match workspace_msrv::parse_or_finding(repo, default_sev, &workspace_msrv_raw) {
            Ok(v) => v,
            Err(finding) => return Ok(finalize(vec![finding])),
        };

    let allowlist = mismatch::build_allowlist(&config.policy.msrv.allow_overrides);
    let allow_per_crate_override = config.policy.msrv.allow_per_crate_override;

    let mut findings = Vec::new();
    for m in &repo.workspace.members {
        let rel = crate::rel_path(&repo.root, &m.manifest_path);

        match member_msrv::resolve(m, &workspace_msrv) {
            member_msrv::Resolution::Missing => {
                findings.push(member_msrv::missing_finding(m, &rel, default_sev));
            }
            member_msrv::Resolution::Invalid(raw) => {
                findings.push(member_msrv::invalid_finding(m, &rel, default_sev, &raw));
            }
            member_msrv::Resolution::Resolved(member_msrv) => {
                if let Some(f) = mismatch::check_member(
                    m,
                    &rel,
                    default_sev,
                    &member_msrv,
                    &workspace_msrv,
                    allow_per_crate_override,
                    &allowlist,
                ) {
                    findings.push(f);
                }
            }
        }
    }

    Ok(finalize(findings))
}

fn skip_report(detail: &str) -> CheckReport {
    CheckReport {
        id: CHECK_ID.to_string(),
        status: CheckStatus::Skip,
        findings: Vec::new(),
        skipped_reason: Some(check_skip_reasons::MISSING_PREREQUISITE.to_string()),
        skipped_detail: Some(detail.to_string()),
    }
}

fn finalize(findings: Vec<Finding>) -> CheckReport {
    CheckReport {
        id: CHECK_ID.to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
        skipped_detail: None,
    }
}
