//! Workspace MSRV parsing and validation.

use builddiag_domain::parse_rust_version;
use builddiag_repo::RepoState;
use builddiag_types::{Finding, Severity};

/// Attempt to parse a workspace MSRV string into its normalized form.
///
/// Returns `Some(normalized)` on success, `None` on parse error.
pub(super) fn parse(raw: &str) -> Option<String> {
    parse_rust_version(raw).ok().map(|v| v.to_string())
}

/// Build an `invalid_msrv` finding for an unparseable workspace MSRV.
pub(super) fn invalid_finding(repo: &RepoState, default_sev: Severity, raw: &str) -> Finding {
    let path = repo
        .cargo_root
        .as_ref()
        .map(|p| crate::rel_path(&repo.root, p));
    crate::mk_finding(
        default_sev,
        super::CHECK_ID,
        "invalid_msrv",
        format!("Invalid workspace MSRV rust-version: {raw}"),
        path,
        None,
    )
}

/// Parse the workspace MSRV or return the `invalid_msrv` finding to surface in the report.
pub(super) fn parse_or_finding(
    repo: &RepoState,
    default_sev: Severity,
    raw: &str,
) -> Result<String, Finding> {
    match parse(raw) {
        Some(normalized) => Ok(normalized),
        None => Err(invalid_finding(repo, default_sev, raw)),
    }
}
