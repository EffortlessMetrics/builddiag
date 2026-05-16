//! MSRV mismatch detection with allowlist support.

use std::collections::HashSet;

use builddiag_repo::Member;
use builddiag_types::{Finding, Severity};

/// Convert the configured allowlist of relative manifest paths into a `HashSet`
/// for O(1) lookup during the per-member comparison.
pub(super) fn build_allowlist(overrides: &[String]) -> HashSet<String> {
    overrides.iter().cloned().collect()
}

/// Compare a member's normalized MSRV to the workspace MSRV.
///
/// Returns `Some(Finding)` with code `msrv_mismatch` only when the versions
/// differ and the mismatch is not allowed by either the global
/// `allow_per_crate_override` toggle or the per-path `allowlist`.
#[allow(clippy::too_many_arguments)]
pub(super) fn check_member(
    member: &Member,
    rel: &str,
    default_sev: Severity,
    member_msrv: &str,
    workspace_msrv: &str,
    allow_per_crate_override: bool,
    allowlist: &HashSet<String>,
) -> Option<Finding> {
    if member_msrv == workspace_msrv {
        return None;
    }
    if allow_per_crate_override || allowlist.contains(rel) {
        return None;
    }
    Some(crate::mk_finding(
        default_sev,
        super::CHECK_ID,
        "msrv_mismatch",
        format!(
            "{}: rust-version {member_msrv} does not match workspace MSRV {workspace_msrv}",
            member.name
        ),
        Some(rel.to_string()),
        None,
    ))
}
