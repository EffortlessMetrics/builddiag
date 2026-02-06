//! Core domain logic for the builddiag build contract validator.
//!
//! This crate provides the fundamental business logic used throughout builddiag:
//!
//! - **Version parsing**: [`parse_rust_version`] normalizes Rust version strings to semver
//! - **Status determination**: [`check_status_from_findings`] derives check status from findings
//! - **Result aggregation**: [`summarize`] combines multiple check results into a summary
//! - **Exit code mapping**: [`exit_code_for`] maps verdicts to process exit codes
//! - **Explain registry**: [`explain`] module provides detailed explanations for checks and codes
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
//!     check_id: "example.check".into(),
//!     code: "example".into(),
//!     severity: Severity::Warn,
//!     message: "Example warning".into(),
//!     location: None,
//! }];
//! let status = check_status_from_findings(&findings);
//! assert_eq!(status, builddiag_types::CheckStatus::Warn);
//! ```

pub mod explain;

use anyhow::{Result, anyhow};
use builddiag_types::{CheckReport, CheckStatus, FailOn, Finding, Severity, Summary, Verdict};
use semver::Version;
use std::collections::BTreeMap;

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
///     check_id: "example.check".into(),
///     code: "example".into(),
///     severity: Severity::Warn,
///     message: "A warning".into(),
///     location: None,
/// }];
/// assert_eq!(check_status_from_findings(&findings), CheckStatus::Warn);
///
/// // Error finding results in fail status (even with warnings)
/// let findings = vec![
///     Finding {
///         check_id: "example.check".into(),
///         code: "warn".into(),
///         severity: Severity::Warn,
///         message: "A warning".into(),
///         location: None,
///     },
///     Finding {
///         check_id: "example.check".into(),
///         code: "error".into(),
///         severity: Severity::Error,
///         message: "An error".into(),
///         location: None,
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
/// A [`Summary`] containing aggregated counts by severity and check.
///
/// # Examples
///
/// ```
/// use builddiag_domain::summarize;
/// use builddiag_types::{CheckReport, CheckStatus, Finding, Severity, Location};
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
/// assert_eq!(summary.total_findings, 0);
///
/// // Summarize with findings
/// let checks = vec![
///     CheckReport {
///         id: "rust.msrv_defined".into(),
///         status: CheckStatus::Fail,
///         findings: vec![Finding {
///             check_id: "rust.msrv_defined".into(),
///             code: "missing_msrv".into(),
///             severity: Severity::Error,
///             message: "Missing rust-version".into(),
///             location: Some(Location {
///                 path: "Cargo.toml".into(),
///                 line: None,
///                 col: None,
///             }),
///         }],
///         skipped_reason: None,
///     },
/// ];
/// let summary = summarize(&checks);
/// assert_eq!(summary.total_findings, 1);
/// assert_eq!(*summary.by_severity.get("error").unwrap_or(&0), 1);
/// ```
pub fn summarize(checks: &[CheckReport]) -> Summary {
    let mut by_severity: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_check: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_findings = 0;

    for c in checks {
        for f in &c.findings {
            total_findings += 1;

            // Count by severity
            let severity_key = match f.severity {
                Severity::Info => "info",
                Severity::Warn => "warn",
                Severity::Error => "error",
            };
            *by_severity.entry(severity_key.to_string()).or_insert(0) += 1;

            // Count by check ID
            *by_check.entry(c.id.clone()).or_insert(0) += 1;
        }
    }

    Summary {
        total_findings,
        by_severity,
        by_check,
    }
}

/// Determine the overall verdict from check reports.
///
/// This function examines all check reports and returns the most severe verdict:
/// - If any check failed, returns `Fail`
/// - If any check warned (and none failed), returns `Warn`
/// - If all checks passed, returns `Pass`
/// - If all checks were skipped, returns `Skip`
///
/// # Arguments
///
/// * `checks` - A slice of check reports to evaluate
///
/// # Returns
///
/// The overall [`Verdict`] based on all check statuses.
pub fn determine_verdict(checks: &[CheckReport]) -> Verdict {
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

    verdict
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
/// | [`Verdict::Warn`] | [`FailOn::Warn`] | `2` |
/// | [`Verdict::Warn`] | [`FailOn::Error`] | `0` |
/// | [`Verdict::Warn`] | [`FailOn::Never`] | `0` |
///
/// # Arguments
///
/// * `verdict` - The verdict to evaluate
/// * `fail_on` - The policy determining when warnings should cause failure
///
/// # Returns
///
/// The process exit code:
/// - `0`: Success (pass, skip, or warnings with lenient policy)
/// - `2`: Policy failure (errors, or warnings with fail_on=warn)
///
/// Note: Exit code `1` is reserved for tool/runtime errors and is handled
/// by the CLI error handling, not by this function.
///
/// # Examples
///
/// ```
/// use builddiag_domain::exit_code_for;
/// use builddiag_types::{Verdict, FailOn};
///
/// // Passing verdict always returns 0
/// assert_eq!(exit_code_for(Verdict::Pass, FailOn::Error), 0);
///
/// // Failing verdict always returns 2
/// assert_eq!(exit_code_for(Verdict::Fail, FailOn::Error), 2);
///
/// // Warning verdict depends on fail_on policy
/// assert_eq!(exit_code_for(Verdict::Warn, FailOn::Warn), 2);
/// assert_eq!(exit_code_for(Verdict::Warn, FailOn::Error), 0);
///
/// // Skip verdict always returns 0
/// assert_eq!(exit_code_for(Verdict::Skip, FailOn::Error), 0);
/// ```
pub fn exit_code_for(verdict: Verdict, fail_on: FailOn) -> i32 {
    match verdict {
        Verdict::Fail | Verdict::Error => 2,
        Verdict::Warn => match fail_on {
            FailOn::Warn => 2,
            FailOn::Error | FailOn::Never => 0,
        },
        Verdict::Pass | Verdict::Skip => 0,
    }
}

/// Sort findings in canonical order for deterministic output.
///
/// This function sorts findings in place using a stable, total ordering that ensures
/// byte-stable output across different runs. The sorting priority is:
///
/// 1. **Severity** (descending): Error > Warn > Info
/// 2. **check_id** (ascending): Alphabetical
/// 3. **location.path** (ascending): Alphabetical, with missing location sorted last
/// 4. **location.line** (ascending): Numeric, with missing line sorted last
/// 5. **code** (ascending): Alphabetical
/// 6. **message** (ascending): Alphabetical
///
/// # Determinism Guarantees
///
/// - **Idempotent**: Sorting twice produces the same result as sorting once
/// - **Deterministic**: Same input always produces the same output
/// - **Total**: Works for any valid `Finding` (no panics, handles all edge cases)
///
/// # Examples
///
/// ```
/// use builddiag_domain::sort_findings_canonical;
/// use builddiag_types::{Finding, Location, Severity};
///
/// let mut findings = vec![
///     Finding {
///         check_id: "rust.msrv".into(),
///         code: "warning_code".into(),
///         severity: Severity::Warn,
///         message: "A warning".into(),
///         location: Some(Location {
///             path: "src/lib.rs".into(),
///             line: Some(10),
///             col: None,
///         }),
///     },
///     Finding {
///         check_id: "rust.msrv".into(),
///         code: "error_code".into(),
///         severity: Severity::Error,
///         message: "An error".into(),
///         location: Some(Location {
///             path: "src/main.rs".into(),
///             line: Some(5),
///             col: None,
///         }),
///     },
/// ];
///
/// sort_findings_canonical(&mut findings);
///
/// // Error comes first (higher severity)
/// assert_eq!(findings[0].severity, Severity::Error);
/// assert_eq!(findings[1].severity, Severity::Warn);
/// ```
pub fn sort_findings_canonical(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        // 1. Severity (descending: Error > Warn > Info)
        // Since Severity's natural order is Info < Warn < Error, we reverse it
        let severity_cmp = b.severity.cmp(&a.severity);
        if severity_cmp != std::cmp::Ordering::Equal {
            return severity_cmp;
        }

        // 2. check_id (ascending)
        let check_id_cmp = a.check_id.cmp(&b.check_id);
        if check_id_cmp != std::cmp::Ordering::Equal {
            return check_id_cmp;
        }

        // 3. location.path (ascending, None last)
        let path_cmp = compare_location_path(&a.location, &b.location);
        if path_cmp != std::cmp::Ordering::Equal {
            return path_cmp;
        }

        // 4. location.line (ascending, None last)
        let line_cmp = compare_location_line(&a.location, &b.location);
        if line_cmp != std::cmp::Ordering::Equal {
            return line_cmp;
        }

        // 5. Code (ascending)
        let code_cmp = a.code.cmp(&b.code);
        if code_cmp != std::cmp::Ordering::Equal {
            return code_cmp;
        }

        // 6. Message (ascending)
        a.message.cmp(&b.message)
    });
}

/// Compare two optional locations by path, with None sorted last.
fn compare_location_path(
    a: &Option<builddiag_types::Location>,
    b: &Option<builddiag_types::Location>,
) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a_loc), Some(b_loc)) => a_loc.path.cmp(&b_loc.path),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Compare two optional locations by line, with None sorted last.
fn compare_location_line(
    a: &Option<builddiag_types::Location>,
    b: &Option<builddiag_types::Location>,
) -> std::cmp::Ordering {
    let a_line = a.as_ref().and_then(|l| l.line);
    let b_line = b.as_ref().and_then(|l| l.line);
    match (a_line, b_line) {
        (Some(a_val), Some(b_val)) => a_val.cmp(&b_val),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

// =============================================================================
// Sensor Report Support (sensor.report.v1)
// =============================================================================

use builddiag_types::{SensorFinding, SensorVerdict, VerdictCounts, VerdictStatus};
use sha2::{Digest, Sha256};

/// Compute a stable fingerprint for a finding.
///
/// The fingerprint is a full SHA-256 hash of the finding's identifying fields:
/// `check_id + code + path + line`. This provides a stable identifier for deduplication
/// across runs, baselines, waivers, and ratchets.
///
/// # Stability Guarantees
///
/// - Same input always produces the same output
/// - Fingerprint is deterministic and portable across platforms
/// - Uses null byte separator to avoid ambiguous concatenation
///
/// # Format
///
/// Returns a 64-character hex string (full SHA-256).
///
/// # Examples
///
/// ```
/// use builddiag_domain::compute_fingerprint;
/// use builddiag_types::{Finding, Severity, Location};
///
/// let finding = Finding {
///     check_id: "rust.msrv_defined".to_string(),
///     code: "missing_msrv".to_string(),
///     severity: Severity::Error,
///     message: "Missing rust-version".to_string(),
///     location: Some(Location {
///         path: "Cargo.toml".to_string(),
///         line: Some(1),
///         col: None,
///     }),
/// };
///
/// let fp1 = compute_fingerprint(&finding);
/// let fp2 = compute_fingerprint(&finding);
/// assert_eq!(fp1, fp2); // Stable
/// assert_eq!(fp1.len(), 64); // Full SHA-256 hex
/// ```
pub fn compute_fingerprint(finding: &Finding) -> String {
    let path = finding
        .location
        .as_ref()
        .map(|l| l.path.as_str())
        .unwrap_or("");
    let line = finding.location.as_ref().and_then(|l| l.line).unwrap_or(0);

    // Use null byte separator to prevent ambiguous concatenation
    let input = format!("{}\0{}\0{}\0{}", finding.check_id, finding.code, path, line);

    let hash = Sha256::digest(input.as_bytes());
    // Return full SHA-256 as 64-char hex string
    hex::encode(hash)
}

/// Convert a base Finding to a SensorFinding with computed fingerprint.
///
/// This converts the builddiag Finding format to the sensor.report.v1 format,
/// computing the fingerprint and optionally adding help text and documentation URL.
///
/// # Arguments
///
/// * `finding` - The base finding to convert
/// * `help` - Optional help text explaining how to fix the issue
/// * `url` - Optional URL to detailed documentation
///
/// # Examples
///
/// ```
/// use builddiag_domain::finding_to_sensor;
/// use builddiag_types::{Finding, Severity};
///
/// let finding = Finding {
///     check_id: "rust.msrv_defined".to_string(),
///     code: "missing_msrv".to_string(),
///     severity: Severity::Error,
///     message: "Missing rust-version".to_string(),
///     location: None,
/// };
///
/// let sensor = finding_to_sensor(&finding, None, None);
/// assert_eq!(sensor.check_id, "rust.msrv_defined");
/// assert!(!sensor.fingerprint.is_empty());
/// ```
pub fn finding_to_sensor(
    finding: &Finding,
    help: Option<String>,
    url: Option<String>,
) -> SensorFinding {
    SensorFinding {
        check_id: finding.check_id.clone(),
        code: finding.code.clone(),
        severity: finding.severity,
        message: finding.message.clone(),
        fingerprint: compute_fingerprint(finding),
        location: finding.location.clone(),
        help,
        url,
        data: None,
    }
}

/// Build verdict counts from a slice of findings.
///
/// Counts findings by severity level. The `suppressed` count is always 0
/// as filtering happens at a higher level.
///
/// # Examples
///
/// ```
/// use builddiag_domain::build_verdict_counts;
/// use builddiag_types::{Finding, Severity};
///
/// let findings = vec![
///     Finding {
///         check_id: "check".to_string(),
///         code: "code".to_string(),
///         severity: Severity::Error,
///         message: "Error".to_string(),
///         location: None,
///     },
///     Finding {
///         check_id: "check".to_string(),
///         code: "code".to_string(),
///         severity: Severity::Warn,
///         message: "Warning".to_string(),
///         location: None,
///     },
/// ];
///
/// let counts = build_verdict_counts(&findings);
/// assert_eq!(counts.error, 1);
/// assert_eq!(counts.warn, 1);
/// assert_eq!(counts.info, 0);
/// ```
pub fn build_verdict_counts(findings: &[Finding]) -> VerdictCounts {
    let mut counts = VerdictCounts::default();
    for f in findings {
        match f.severity {
            Severity::Info => counts.info += 1,
            Severity::Warn => counts.warn += 1,
            Severity::Error => counts.error += 1,
        }
    }
    counts
}

/// Build verdict reasons from check reports.
///
/// Returns a list of machine-addressable reason tokens explaining why the
/// verdict is not Pass. Tokens are coarse (one per category) for cockpit
/// policy routing; per-check detail lives in [`build_verdict_data`].
///
/// # Token Format
///
/// - `all_checks_skipped` — every check was skipped
/// - `tool_error` — an internal error occurred
/// - `checks_failed` — one or more checks failed
/// - `checks_warned` — one or more checks warned
///
/// # Examples
///
/// ```
/// use builddiag_domain::build_verdict_reasons;
/// use builddiag_types::{CheckReport, CheckStatus, Verdict};
///
/// let checks = vec![
///     CheckReport {
///         id: "rust.msrv_defined".to_string(),
///         status: CheckStatus::Fail,
///         findings: vec![],
///         skipped_reason: None,
///     },
/// ];
///
/// let reasons = build_verdict_reasons(Verdict::Fail, &checks);
/// assert!(reasons.contains(&"checks_failed".to_string()));
/// ```
pub fn build_verdict_reasons(verdict: Verdict, checks: &[CheckReport]) -> Vec<String> {
    let mut reasons = Vec::new();

    match verdict {
        Verdict::Pass => {}
        Verdict::Skip => {
            reasons.push("all_checks_skipped".to_string());
        }
        Verdict::Error => {
            reasons.push("tool_error".to_string());
        }
        Verdict::Warn | Verdict::Fail => {
            let has_failed = checks.iter().any(|c| c.status == CheckStatus::Fail);
            let has_warned = checks.iter().any(|c| c.status == CheckStatus::Warn);

            if has_failed {
                reasons.push("checks_failed".to_string());
            }
            if has_warned {
                reasons.push("checks_warned".to_string());
            }
        }
    }

    reasons
}

/// Build verdict data with per-check detail behind coarse reason tokens.
///
/// Returns `Some(Value)` with `failed_checks` and/or `warned_checks` arrays
/// when checks have failed or warned. Returns `None` for Pass/Skip/Error
/// verdicts or when no check-level detail is available.
///
/// # Examples
///
/// ```
/// use builddiag_domain::build_verdict_data;
/// use builddiag_types::{CheckReport, CheckStatus, Verdict};
///
/// let checks = vec![
///     CheckReport {
///         id: "rust.msrv_defined".to_string(),
///         status: CheckStatus::Fail,
///         findings: vec![],
///         skipped_reason: None,
///     },
/// ];
///
/// let data = build_verdict_data(Verdict::Fail, &checks).unwrap();
/// let failed = data["failed_checks"].as_array().unwrap();
/// assert_eq!(failed[0].as_str().unwrap(), "rust.msrv_defined");
/// ```
pub fn build_verdict_data(verdict: Verdict, checks: &[CheckReport]) -> Option<serde_json::Value> {
    match verdict {
        Verdict::Warn | Verdict::Fail => {
            let mut failed: Vec<String> = checks
                .iter()
                .filter(|c| c.status == CheckStatus::Fail)
                .map(|c| c.id.clone())
                .collect();
            failed.sort();
            let mut warned: Vec<String> = checks
                .iter()
                .filter(|c| c.status == CheckStatus::Warn)
                .map(|c| c.id.clone())
                .collect();
            warned.sort();

            if failed.is_empty() && warned.is_empty() {
                return None;
            }

            let mut map = serde_json::Map::new();
            if !failed.is_empty() {
                map.insert("failed_checks".to_string(), serde_json::json!(failed));
            }
            if !warned.is_empty() {
                map.insert("warned_checks".to_string(), serde_json::json!(warned));
            }
            Some(serde_json::Value::Object(map))
        }
        _ => None,
    }
}

/// Build a complete SensorVerdict from a Verdict and check reports.
///
/// Combines the verdict status, finding counts, reasons, and per-check
/// detail data into the sensor.report.v1 verdict structure.
///
/// # Examples
///
/// ```
/// use builddiag_domain::build_sensor_verdict;
/// use builddiag_types::{CheckReport, CheckStatus, Finding, Severity, Verdict, VerdictStatus};
///
/// let checks = vec![
///     CheckReport {
///         id: "rust.msrv_defined".to_string(),
///         status: CheckStatus::Fail,
///         findings: vec![Finding {
///             check_id: "rust.msrv_defined".to_string(),
///             code: "missing_msrv".to_string(),
///             severity: Severity::Error,
///             message: "Missing MSRV".to_string(),
///             location: None,
///         }],
///         skipped_reason: None,
///     },
/// ];
///
/// let verdict = build_sensor_verdict(Verdict::Fail, &checks);
/// assert_eq!(verdict.status, VerdictStatus::Fail);
/// assert_eq!(verdict.counts.error, 1);
/// assert!(!verdict.reasons.is_empty());
/// assert!(verdict.data.is_some());
/// ```
pub fn build_sensor_verdict(verdict: Verdict, checks: &[CheckReport]) -> SensorVerdict {
    // Collect all findings from checks
    let all_findings: Vec<&Finding> = checks.iter().flat_map(|c| c.findings.iter()).collect();

    // Build counts from findings (need to convert refs to owned for count function)
    let findings_owned: Vec<Finding> = all_findings.into_iter().cloned().collect();
    let counts = build_verdict_counts(&findings_owned);

    SensorVerdict {
        status: VerdictStatus::from(verdict),
        counts,
        reasons: build_verdict_reasons(verdict, checks),
        data: build_verdict_data(verdict, checks),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{Finding, Location, Severity};

    /// Helper to create a Finding for tests
    fn make_finding(
        severity: Severity,
        check_id: &str,
        code: &str,
        message: &str,
        path: Option<&str>,
        line: Option<u32>,
    ) -> Finding {
        Finding {
            check_id: check_id.into(),
            code: code.into(),
            severity,
            message: message.into(),
            location: path.map(|p| Location {
                path: p.into(),
                line,
                col: None,
            }),
        }
    }

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

        let warn = vec![make_finding(Severity::Warn, "check", "x", "x", None, None)];
        assert_eq!(check_status_from_findings(&warn), CheckStatus::Warn);

        let err = vec![make_finding(Severity::Error, "check", "x", "x", None, None)];
        assert_eq!(check_status_from_findings(&err), CheckStatus::Fail);
    }

    #[test]
    fn test_sort_findings_canonical_empty() {
        let mut findings: Vec<Finding> = vec![];
        sort_findings_canonical(&mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_sort_findings_canonical_single() {
        let mut findings = vec![make_finding(
            Severity::Warn,
            "check",
            "test",
            "message",
            Some("file.rs"),
            Some(10),
        )];
        let original = findings.clone();
        sort_findings_canonical(&mut findings);
        assert_eq!(findings, original);
    }

    #[test]
    fn test_sort_findings_canonical_by_severity() {
        let mut findings = vec![
            make_finding(Severity::Info, "check", "a", "info", None, None),
            make_finding(Severity::Error, "check", "a", "error", None, None),
            make_finding(Severity::Warn, "check", "a", "warn", None, None),
        ];
        sort_findings_canonical(&mut findings);

        // Error > Warn > Info
        assert_eq!(findings[0].severity, Severity::Error);
        assert_eq!(findings[1].severity, Severity::Warn);
        assert_eq!(findings[2].severity, Severity::Info);
    }

    #[test]
    fn test_sort_findings_canonical_by_check_id() {
        let mut findings = vec![
            make_finding(Severity::Error, "zulu", "a", "m", None, None),
            make_finding(Severity::Error, "alpha", "a", "m", None, None),
            make_finding(Severity::Error, "mike", "a", "m", None, None),
        ];
        sort_findings_canonical(&mut findings);

        // Alphabetical by check_id
        assert_eq!(findings[0].check_id, "alpha");
        assert_eq!(findings[1].check_id, "mike");
        assert_eq!(findings[2].check_id, "zulu");
    }

    #[test]
    fn test_sort_findings_canonical_by_path() {
        let mut findings = vec![
            make_finding(Severity::Error, "check", "a", "m", Some("z.rs"), None),
            make_finding(Severity::Error, "check", "a", "m", Some("a.rs"), None),
            make_finding(Severity::Error, "check", "a", "m", None, None),
        ];
        sort_findings_canonical(&mut findings);

        // Alphabetical, None last
        assert_eq!(
            findings[0].location.as_ref().map(|l| l.path.as_str()),
            Some("a.rs")
        );
        assert_eq!(
            findings[1].location.as_ref().map(|l| l.path.as_str()),
            Some("z.rs")
        );
        assert!(findings[2].location.is_none());
    }

    #[test]
    fn test_sort_findings_canonical_by_line() {
        let mut findings = vec![
            make_finding(
                Severity::Error,
                "check",
                "a",
                "m",
                Some("file.rs"),
                Some(100),
            ),
            make_finding(
                Severity::Error,
                "check",
                "a",
                "m",
                Some("file.rs"),
                Some(10),
            ),
            make_finding(Severity::Error, "check", "a", "m", Some("file.rs"), None),
        ];
        sort_findings_canonical(&mut findings);

        // Numeric ascending, None last
        assert_eq!(findings[0].location.as_ref().and_then(|l| l.line), Some(10));
        assert_eq!(
            findings[1].location.as_ref().and_then(|l| l.line),
            Some(100)
        );
        assert_eq!(findings[2].location.as_ref().and_then(|l| l.line), None);
    }

    #[test]
    fn test_sort_findings_canonical_by_code() {
        let mut findings = vec![
            make_finding(
                Severity::Error,
                "check",
                "zebra",
                "m",
                Some("file.rs"),
                Some(10),
            ),
            make_finding(
                Severity::Error,
                "check",
                "apple",
                "m",
                Some("file.rs"),
                Some(10),
            ),
        ];
        sort_findings_canonical(&mut findings);

        assert_eq!(findings[0].code, "apple");
        assert_eq!(findings[1].code, "zebra");
    }

    #[test]
    fn test_sort_findings_canonical_by_message() {
        let mut findings = vec![
            make_finding(
                Severity::Error,
                "check",
                "code",
                "zebra message",
                Some("file.rs"),
                Some(10),
            ),
            make_finding(
                Severity::Error,
                "check",
                "code",
                "apple message",
                Some("file.rs"),
                Some(10),
            ),
        ];
        sort_findings_canonical(&mut findings);

        assert_eq!(findings[0].message, "apple message");
        assert_eq!(findings[1].message, "zebra message");
    }

    #[test]
    fn test_sort_findings_canonical_idempotent() {
        let mut findings = vec![
            make_finding(Severity::Warn, "check", "b", "msg", Some("z.rs"), Some(5)),
            make_finding(Severity::Error, "check", "a", "msg", Some("a.rs"), Some(10)),
        ];

        sort_findings_canonical(&mut findings);
        let after_first_sort = findings.clone();

        sort_findings_canonical(&mut findings);
        assert_eq!(findings, after_first_sort);
    }

    // ==========================================================================
    // Sensor report support tests
    // ==========================================================================

    #[test]
    fn test_compute_fingerprint_stability() {
        let finding = make_finding(
            Severity::Error,
            "rust.msrv_defined",
            "missing_msrv",
            "Missing rust-version",
            Some("Cargo.toml"),
            Some(1),
        );

        let fp1 = compute_fingerprint(&finding);
        let fp2 = compute_fingerprint(&finding);

        assert_eq!(fp1, fp2, "Fingerprint should be stable");
        assert_eq!(
            fp1.len(),
            64,
            "Fingerprint should be 64 hex chars (full SHA-256)"
        );
    }

    #[test]
    fn test_compute_fingerprint_different_findings() {
        let finding1 = make_finding(
            Severity::Error,
            "rust.msrv_defined",
            "missing_msrv",
            "Missing rust-version",
            Some("Cargo.toml"),
            Some(1),
        );

        let finding2 = make_finding(
            Severity::Error,
            "rust.msrv_defined",
            "missing_msrv",
            "Missing rust-version",
            Some("Cargo.toml"),
            Some(2), // Different line
        );

        let fp1 = compute_fingerprint(&finding1);
        let fp2 = compute_fingerprint(&finding2);

        assert_ne!(
            fp1, fp2,
            "Different findings should have different fingerprints"
        );
    }

    #[test]
    fn test_compute_fingerprint_ignores_message() {
        let finding1 = make_finding(
            Severity::Error,
            "check",
            "code",
            "Message 1",
            Some("file.rs"),
            Some(10),
        );

        let finding2 = make_finding(
            Severity::Error,
            "check",
            "code",
            "Message 2", // Different message
            Some("file.rs"),
            Some(10),
        );

        let fp1 = compute_fingerprint(&finding1);
        let fp2 = compute_fingerprint(&finding2);

        assert_eq!(fp1, fp2, "Message should not affect fingerprint");
    }

    #[test]
    fn test_compute_fingerprint_no_location() {
        let finding = make_finding(Severity::Warn, "check", "code", "msg", None, None);

        let fp = compute_fingerprint(&finding);
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn test_finding_to_sensor() {
        let finding = make_finding(
            Severity::Error,
            "rust.msrv_defined",
            "missing_msrv",
            "Missing rust-version",
            Some("Cargo.toml"),
            Some(1),
        );

        let sensor = finding_to_sensor(
            &finding,
            Some("Add rust-version to [package]".to_string()),
            Some("https://docs.example.com/msrv".to_string()),
        );

        assert_eq!(sensor.check_id, "rust.msrv_defined");
        assert_eq!(sensor.code, "missing_msrv");
        assert_eq!(sensor.severity, Severity::Error);
        assert!(!sensor.fingerprint.is_empty());
        assert_eq!(
            sensor.help,
            Some("Add rust-version to [package]".to_string())
        );
        assert_eq!(
            sensor.url,
            Some("https://docs.example.com/msrv".to_string())
        );
    }

    #[test]
    fn test_build_verdict_counts_empty() {
        let counts = build_verdict_counts(&[]);
        assert_eq!(counts.info, 0);
        assert_eq!(counts.warn, 0);
        assert_eq!(counts.error, 0);
        assert_eq!(counts.suppressed, 0);
    }

    #[test]
    fn test_build_verdict_counts_mixed() {
        let findings = vec![
            make_finding(Severity::Info, "c", "x", "m", None, None),
            make_finding(Severity::Warn, "c", "x", "m", None, None),
            make_finding(Severity::Warn, "c", "x", "m", None, None),
            make_finding(Severity::Error, "c", "x", "m", None, None),
            make_finding(Severity::Error, "c", "x", "m", None, None),
            make_finding(Severity::Error, "c", "x", "m", None, None),
        ];

        let counts = build_verdict_counts(&findings);
        assert_eq!(counts.info, 1);
        assert_eq!(counts.warn, 2);
        assert_eq!(counts.error, 3);
    }

    #[test]
    fn test_build_verdict_reasons_pass() {
        let checks = vec![CheckReport {
            id: "check".to_string(),
            status: CheckStatus::Pass,
            findings: vec![],
            skipped_reason: None,
        }];

        let reasons = build_verdict_reasons(Verdict::Pass, &checks);
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_build_verdict_reasons_fail() {
        let checks = vec![
            CheckReport {
                id: "rust.msrv_defined".to_string(),
                status: CheckStatus::Fail,
                findings: vec![],
                skipped_reason: None,
            },
            CheckReport {
                id: "rust.toolchain".to_string(),
                status: CheckStatus::Warn,
                findings: vec![],
                skipped_reason: None,
            },
        ];

        let reasons = build_verdict_reasons(Verdict::Fail, &checks);
        assert!(reasons.contains(&"checks_failed".to_string()));
        assert!(reasons.contains(&"checks_warned".to_string()));
        assert_eq!(reasons.len(), 2);
    }

    #[test]
    fn test_build_verdict_reasons_skip() {
        let checks = vec![];
        let reasons = build_verdict_reasons(Verdict::Skip, &checks);
        assert!(reasons.contains(&"all_checks_skipped".to_string()));
    }

    #[test]
    fn test_build_verdict_reasons_error() {
        let checks = vec![];
        let reasons = build_verdict_reasons(Verdict::Error, &checks);
        assert!(reasons.contains(&"tool_error".to_string()));
    }

    #[test]
    fn test_build_sensor_verdict() {
        let checks = vec![CheckReport {
            id: "rust.msrv_defined".to_string(),
            status: CheckStatus::Fail,
            findings: vec![make_finding(
                Severity::Error,
                "rust.msrv_defined",
                "missing_msrv",
                "Missing",
                None,
                None,
            )],
            skipped_reason: None,
        }];

        let verdict = build_sensor_verdict(Verdict::Fail, &checks);
        assert_eq!(verdict.status, VerdictStatus::Fail);
        assert_eq!(verdict.counts.error, 1);
        assert!(verdict.reasons.contains(&"checks_failed".to_string()));
        let data = verdict.data.as_ref().unwrap();
        let failed = data["failed_checks"].as_array().unwrap();
        assert_eq!(failed[0].as_str().unwrap(), "rust.msrv_defined");
    }

    #[test]
    fn test_build_verdict_data_fail_and_warn() {
        let checks = vec![
            CheckReport {
                id: "rust.msrv_defined".to_string(),
                status: CheckStatus::Fail,
                findings: vec![],
                skipped_reason: None,
            },
            CheckReport {
                id: "rust.toolchain".to_string(),
                status: CheckStatus::Warn,
                findings: vec![],
                skipped_reason: None,
            },
        ];

        let data = build_verdict_data(Verdict::Fail, &checks).unwrap();
        let failed = data["failed_checks"].as_array().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].as_str().unwrap(), "rust.msrv_defined");
        let warned = data["warned_checks"].as_array().unwrap();
        assert_eq!(warned.len(), 1);
        assert_eq!(warned[0].as_str().unwrap(), "rust.toolchain");
    }

    #[test]
    fn test_build_verdict_data_pass_returns_none() {
        let checks = vec![CheckReport {
            id: "check".to_string(),
            status: CheckStatus::Pass,
            findings: vec![],
            skipped_reason: None,
        }];

        assert!(build_verdict_data(Verdict::Pass, &checks).is_none());
    }

    #[test]
    fn test_build_verdict_data_skip_returns_none() {
        let checks = vec![];
        assert!(build_verdict_data(Verdict::Skip, &checks).is_none());
    }
}
