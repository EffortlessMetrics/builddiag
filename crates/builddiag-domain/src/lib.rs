//! Core domain logic for the builddiag build contract validator.
//!
//! This crate provides the fundamental business logic used throughout builddiag:
//!
//! - **Version parsing**: [`parse_rust_version`] normalizes Rust version strings to semver
//! - **Status determination**: [`check_status_from_findings`] derives check status from findings
//! - **Result aggregation**: [`summarize`] combines multiple check results into a summary
//! - **Exit code mapping**: [`exit_code_for`] maps verdicts to process exit codes
//!
//! # Architecture
//!
//! This crate sits at the bottom of the dependency hierarchy, depending only on
//! [`builddiag_types`]. It contains pure functions with no I/O, making it easy to test.
//!
//! # Example
//!
//! ```
//! use builddiag_domain::{parse_rust_version, check_status_from_findings};
//! use builddiag_types::{Finding, Severity};
//!
//! // Parse a Rust version string
//! let version = parse_rust_version("1.75").unwrap();
//! assert_eq!(version.to_string(), "1.75.0");
//!
//! // Determine status from findings
//! let findings = vec![Finding {
//!     severity: Severity::Warn,
//!     code: "example".into(),
//!     message: "Example warning".into(),
//!     path: None,
//!     line: None,
//!     column: None,
//! }];
//! let status = check_status_from_findings(&findings);
//! assert_eq!(status, builddiag_types::CheckStatus::Warn);
//! ```

use anyhow::{Result, anyhow};
use builddiag_types::{
    CheckReport, CheckStatus, FailOn, Finding, Severity, Summary, SummaryCounts, Verdict,
};
use semver::Version;

/// Parse a Rust toolchain or MSRV version string into a semver [`Version`].
///
/// This function normalizes various Rust version formats into a full three-component
/// semver version. It handles the common patterns found in `Cargo.toml` `rust-version`
/// fields and `rust-toolchain.toml` channel specifications.
///
/// # Supported Formats
///
/// - Single component: `"1"` → `1.0.0`
/// - Two components: `"1.75"` → `1.75.0`
/// - Full semver: `"1.75.0"` → `1.75.0`
///
/// # Errors
///
/// Returns an error if:
/// - The input is empty or contains only whitespace
/// - The input cannot be parsed as a valid semver version
///
/// # Examples
///
/// ```
/// use builddiag_domain::parse_rust_version;
///
/// // Two-component version (common in Cargo.toml)
/// let v = parse_rust_version("1.75").unwrap();
/// assert_eq!(v.major, 1);
/// assert_eq!(v.minor, 75);
/// assert_eq!(v.patch, 0);
///
/// // Full semver version
/// let v = parse_rust_version("1.75.1").unwrap();
/// assert_eq!(v.patch, 1);
///
/// // Whitespace is trimmed
/// let v = parse_rust_version("  1.70  ").unwrap();
/// assert_eq!(v.to_string(), "1.70.0");
///
/// // Empty input returns an error
/// assert!(parse_rust_version("").is_err());
/// ```
pub fn parse_rust_version(input: &str) -> Result<Version> {
    let s = input.trim();
    if s.is_empty() {
        return Err(anyhow!("empty version"));
    }

    // Semver expects three components.
    let parts: Vec<&str> = s.split('.').collect();
    let normalized = match parts.len() {
        1 => format!("{}.0.0", s),
        2 => format!("{}.0", s),
        _ => s.to_string(),
    };

    Version::parse(&normalized).map_err(|e| anyhow!("invalid version '{s}': {e}"))
}

/// Determine the [`CheckStatus`] based on the severity of findings.
///
/// This function examines a slice of [`Finding`]s and returns the appropriate
/// [`CheckStatus`] based on the highest severity level present:
///
/// - If any finding has [`Severity::Error`], returns [`CheckStatus::Fail`]
/// - If any finding has [`Severity::Warn`] (and no errors), returns [`CheckStatus::Warn`]
/// - Otherwise (empty or only [`Severity::Info`]), returns [`CheckStatus::Pass`]
///
/// # Arguments
///
/// * `findings` - A slice of findings to evaluate
///
/// # Returns
///
/// The [`CheckStatus`] corresponding to the highest severity finding.
///
/// # Examples
///
/// ```
/// use builddiag_domain::check_status_from_findings;
/// use builddiag_types::{Finding, Severity, CheckStatus};
///
/// // No findings means pass
/// let status = check_status_from_findings(&[]);
/// assert_eq!(status, CheckStatus::Pass);
///
/// // Warning finding results in warn status
/// let findings = vec![Finding {
///     severity: Severity::Warn,
///     code: "example".into(),
///     message: "A warning".into(),
///     path: None,
///     line: None,
///     column: None,
/// }];
/// assert_eq!(check_status_from_findings(&findings), CheckStatus::Warn);
///
/// // Error finding results in fail status (even with warnings)
/// let findings = vec![
///     Finding {
///         severity: Severity::Warn,
///         code: "warn".into(),
///         message: "A warning".into(),
///         path: None,
///         line: None,
///         column: None,
///     },
///     Finding {
///         severity: Severity::Error,
///         code: "error".into(),
///         message: "An error".into(),
///         path: None,
///         line: None,
///         column: None,
///     },
/// ];
/// assert_eq!(check_status_from_findings(&findings), CheckStatus::Fail);
/// ```
pub fn check_status_from_findings(findings: &[Finding]) -> CheckStatus {
    if findings.iter().any(|f| f.severity == Severity::Error) {
        CheckStatus::Fail
    } else if findings.iter().any(|f| f.severity == Severity::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    }
}

/// Aggregate multiple check results into a [`Summary`].
///
/// This function combines the results from multiple [`CheckReport`]s into a single
/// [`Summary`] that provides:
///
/// - **Counts**: Total number of info, warning, and error findings across all checks
/// - **Verdict**: Overall pass/warn/fail/skip status based on check statuses
/// - **Reasons**: List of check IDs that contributed to warn or fail verdicts
///
/// # Verdict Determination
///
/// The overall verdict is determined by the highest severity status across all checks:
///
/// | Check Statuses | Verdict |
/// |----------------|---------|
/// | All Skip | [`Verdict::Skip`] |
/// | All Pass (or Skip) | [`Verdict::Pass`] |
/// | Any Warn (no Fail) | [`Verdict::Warn`] |
/// | Any Fail | [`Verdict::Fail`] |
///
/// # Arguments
///
/// * `checks` - A slice of check reports to summarize
///
/// # Returns
///
/// A [`Summary`] containing aggregated counts, overall verdict, and failure reasons.
///
/// # Examples
///
/// ```
/// use builddiag_domain::summarize;
/// use builddiag_types::{CheckReport, CheckStatus, Finding, Severity, Verdict};
///
/// // Summarize passing checks
/// let checks = vec![
///     CheckReport {
///         id: "check1".into(),
///         status: CheckStatus::Pass,
///         findings: vec![],
///         skipped_reason: None,
///     },
///     CheckReport {
///         id: "check2".into(),
///         status: CheckStatus::Pass,
///         findings: vec![],
///         skipped_reason: None,
///     },
/// ];
/// let summary = summarize(&checks);
/// assert_eq!(summary.verdict, Verdict::Pass);
/// assert!(summary.reasons.is_empty());
///
/// // Summarize with a failing check
/// let checks = vec![
///     CheckReport {
///         id: "msrv".into(),
///         status: CheckStatus::Fail,
///         findings: vec![Finding {
///             severity: Severity::Error,
///             code: "missing_msrv".into(),
///             message: "Missing rust-version".into(),
///             path: Some("Cargo.toml".into()),
///             line: None,
///             column: None,
///         }],
///         skipped_reason: None,
///     },
/// ];
/// let summary = summarize(&checks);
/// assert_eq!(summary.verdict, Verdict::Fail);
/// assert_eq!(summary.counts.error, 1);
/// assert!(summary.reasons.contains(&"msrv: fail".to_string()));
/// ```
pub fn summarize(checks: &[CheckReport]) -> Summary {
    let mut counts = SummaryCounts {
        info: 0,
        warn: 0,
        error: 0,
    };

    for c in checks {
        for f in &c.findings {
            match f.severity {
                Severity::Info => counts.info += 1,
                Severity::Warn => counts.warn += 1,
                Severity::Error => counts.error += 1,
            }
        }
    }

    let mut verdict = Verdict::Pass;
    let mut any_ran = false;

    for c in checks {
        match c.status {
            CheckStatus::Skip => {}
            CheckStatus::Pass => any_ran = true,
            CheckStatus::Warn => {
                any_ran = true;
                if verdict != Verdict::Fail {
                    verdict = Verdict::Warn;
                }
            }
            CheckStatus::Fail => {
                any_ran = true;
                verdict = Verdict::Fail;
            }
        }
    }

    if !any_ran {
        verdict = Verdict::Skip;
    }

    let mut reasons = Vec::new();
    for c in checks {
        match c.status {
            CheckStatus::Fail => reasons.push(format!("{}: fail", c.id)),
            CheckStatus::Warn => reasons.push(format!("{}: warn", c.id)),
            _ => {}
        }
    }

    Summary {
        counts,
        verdict,
        reasons,
    }
}

/// Determine the process exit code based on the summary verdict and fail policy.
///
/// This function maps the [`Summary`] verdict to an appropriate process exit code,
/// taking into account the configured [`FailOn`] policy for handling warnings.
///
/// # Exit Code Mapping
///
/// | Verdict | FailOn Policy | Exit Code |
/// |---------|---------------|-----------|
/// | [`Verdict::Pass`] | Any | `0` |
/// | [`Verdict::Skip`] | Any | `0` |
/// | [`Verdict::Fail`] | Any | `2` |
/// | [`Verdict::Warn`] | [`FailOn::Warn`] | `3` |
/// | [`Verdict::Warn`] | [`FailOn::Error`] | `0` |
/// | [`Verdict::Warn`] | [`FailOn::Never`] | `0` |
///
/// # Arguments
///
/// * `summary` - The summary containing the verdict to evaluate
/// * `fail_on` - The policy determining when warnings should cause failure
///
/// # Returns
///
/// The process exit code:
/// - `0`: Success (pass, skip, or warnings with lenient policy)
/// - `2`: Failure (at least one check failed)
/// - `3`: Warning treated as failure (warnings with strict policy)
///
/// # Examples
///
/// ```
/// use builddiag_domain::exit_code_for;
/// use builddiag_types::{Summary, SummaryCounts, Verdict, FailOn};
///
/// let passing = Summary {
///     counts: SummaryCounts { info: 0, warn: 0, error: 0 },
///     verdict: Verdict::Pass,
///     reasons: vec![],
/// };
/// assert_eq!(exit_code_for(&passing, FailOn::Error), 0);
///
/// let failing = Summary {
///     counts: SummaryCounts { info: 0, warn: 0, error: 1 },
///     verdict: Verdict::Fail,
///     reasons: vec!["check: fail".into()],
/// };
/// assert_eq!(exit_code_for(&failing, FailOn::Error), 2);
///
/// let warning = Summary {
///     counts: SummaryCounts { info: 0, warn: 1, error: 0 },
///     verdict: Verdict::Warn,
///     reasons: vec!["check: warn".into()],
/// };
/// // With FailOn::Warn, warnings cause exit code 3
/// assert_eq!(exit_code_for(&warning, FailOn::Warn), 3);
/// // With FailOn::Error, warnings are allowed (exit code 0)
/// assert_eq!(exit_code_for(&warning, FailOn::Error), 0);
/// ```
pub fn exit_code_for(summary: &Summary, fail_on: FailOn) -> i32 {
    match summary.verdict {
        Verdict::Fail => 2,
        Verdict::Warn => match fail_on {
            FailOn::Warn => 3,
            FailOn::Error | FailOn::Never => 0,
        },
        Verdict::Pass | Verdict::Skip => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{Finding, Severity};

    #[test]
    fn parse_versions() {
        assert_eq!(parse_rust_version("1").unwrap().to_string(), "1.0.0");
        assert_eq!(parse_rust_version("1.75").unwrap().to_string(), "1.75.0");
        assert_eq!(parse_rust_version("1.75.0").unwrap().to_string(), "1.75.0");
        assert!(parse_rust_version("").is_err());
    }

    #[test]
    fn status_from_findings() {
        let ok: Vec<Finding> = Vec::new();
        assert_eq!(check_status_from_findings(&ok), CheckStatus::Pass);

        let warn = vec![Finding {
            severity: Severity::Warn,
            code: "x".into(),
            message: "x".into(),
            path: None,
            line: None,
            column: None,
        }];
        assert_eq!(check_status_from_findings(&warn), CheckStatus::Warn);

        let err = vec![Finding {
            severity: Severity::Error,
            code: "x".into(),
            message: "x".into(),
            path: None,
            line: None,
            column: None,
        }];
        assert_eq!(check_status_from_findings(&err), CheckStatus::Fail);
    }
}
