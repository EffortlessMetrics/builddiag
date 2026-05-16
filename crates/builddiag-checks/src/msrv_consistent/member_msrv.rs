//! Per-member effective MSRV resolution and parsing.

use builddiag_domain::parse_rust_version;
use builddiag_repo::Member;
use builddiag_types::{Finding, Severity};

/// Outcome of resolving a member's effective MSRV.
pub(super) enum Resolution {
    /// The member has no MSRV and does not inherit from the workspace.
    Missing,
    /// The member declares an MSRV but it failed to parse (raw value preserved).
    Invalid(String),
    /// The member's MSRV (already normalized) is available for comparison.
    Resolved(String),
}

/// Determine the effective MSRV for `member` given the (already normalized)
/// `workspace_msrv`.
pub(super) fn resolve(member: &Member, workspace_msrv: &str) -> Resolution {
    if let Some(rv) = &member.rust_version {
        match parse_rust_version(rv) {
            Ok(v) => Resolution::Resolved(v.to_string()),
            Err(_) => Resolution::Invalid(rv.clone()),
        }
    } else if member.rust_version_workspace {
        Resolution::Resolved(workspace_msrv.to_string())
    } else {
        Resolution::Missing
    }
}

/// Build a `missing_member_msrv` finding for a member with no MSRV declared
/// and no inheritance from the workspace.
pub(super) fn missing_finding(member: &Member, rel: &str, default_sev: Severity) -> Finding {
    crate::mk_finding(
        default_sev,
        super::CHECK_ID,
        "missing_member_msrv",
        format!(
            "{}: missing package.rust-version (and not set to inherit from workspace)",
            member.name
        ),
        Some(rel.to_string()),
        None,
    )
}

/// Build an `invalid_member_msrv` finding for a member whose declared MSRV is
/// unparseable.
pub(super) fn invalid_finding(
    member: &Member,
    rel: &str,
    default_sev: Severity,
    raw: &str,
) -> Finding {
    crate::mk_finding(
        default_sev,
        super::CHECK_ID,
        "invalid_member_msrv",
        format!("{}: invalid rust-version '{raw}'", member.name),
        Some(rel.to_string()),
        None,
    )
}
