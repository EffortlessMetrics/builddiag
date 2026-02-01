//! Property-based tests for builddiag-domain.
//!
//! This module contains property tests that validate universal invariants
//! for the core domain logic in builddiag-domain, including:
//!
//! - Version parsing and normalization
//! - Check status determination from findings
//! - Summary aggregation from check reports
//!
//! # Properties Tested
//!
//! - **Property 8**: Version Parsing Normalization (Requirements 3.1)
//! - **Property 6**: Check Status Consistency (Requirements 3.2)
//! - **Property 7**: Summary Aggregation Consistency (Requirements 3.3)

use builddiag_domain::{check_status_from_findings, parse_rust_version, summarize};
use builddiag_types::{CheckReport, CheckStatus, Finding, Severity, Verdict};
use proptest::prelude::*;

// =============================================================================
// Proptest Configuration
// =============================================================================

/// Configure proptest to run at least 100 iterations per property
/// as specified in the design document.
const PROPTEST_CASES: u32 = 100;

// =============================================================================
// Arbitrary Generators
// =============================================================================

/// Generate arbitrary non-empty strings suitable for identifiers.
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,20}".prop_map(|s| s.to_string())
}

/// Generate arbitrary human-readable messages.
fn arb_message() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?-]{1,100}".prop_map(|s| s.to_string())
}

/// Generate arbitrary file paths.
fn arb_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/]{0,30}\\.(toml|rs|md)".prop_map(|s| s.to_string())
}

/// Generate arbitrary Severity values.
fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

/// Generate arbitrary CheckStatus values.
fn arb_check_status() -> impl Strategy<Value = CheckStatus> {
    prop_oneof![
        Just(CheckStatus::Pass),
        Just(CheckStatus::Warn),
        Just(CheckStatus::Fail),
        Just(CheckStatus::Skip),
    ]
}

/// Generate arbitrary Finding instances.
fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_severity(),
        arb_identifier(),
        arb_message(),
        proptest::option::of(arb_path()),
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(|(severity, code, message, path, line, column)| Finding {
            severity,
            code,
            message,
            path,
            line,
            column,
        })
}

/// Generate arbitrary CheckReport instances.
fn arb_check_report() -> impl Strategy<Value = CheckReport> {
    (
        arb_identifier(),
        arb_check_status(),
        proptest::collection::vec(arb_finding(), 0..5),
        proptest::option::of(arb_message()),
    )
        .prop_map(|(id, status, findings, skipped_reason)| CheckReport {
            id,
            status,
            findings,
            skipped_reason,
        })
}

/// Generate valid Rust version strings in various formats.
///
/// This generator produces version strings that should be parseable:
/// - Single component: "1", "2"
/// - Two components: "1.75", "1.80"
/// - Three components: "1.75.0", "1.80.1"
fn arb_valid_rust_version() -> impl Strategy<Value = String> {
    prop_oneof![
        // Single component: "1" or "2"
        (1u32..=2).prop_map(|maj| format!("{}", maj)),
        // Two components: "1.50" to "1.85"
        (1u32..=2, 50u32..=85).prop_map(|(maj, min)| format!("{}.{}", maj, min)),
        // Three components: full semver
        (1u32..=2, 50u32..=85, 0u32..=10)
            .prop_map(|(maj, min, pat)| format!("{}.{}.{}", maj, min, pat)),
    ]
}

/// Generate Finding instances with a specific severity.
/// This function is available for future use in more targeted property tests.
#[allow(dead_code)]
fn arb_finding_with_severity(severity: Severity) -> impl Strategy<Value = Finding> {
    (
        arb_identifier(),
        arb_message(),
        proptest::option::of(arb_path()),
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(move |(code, message, path, line, column)| Finding {
            severity,
            code,
            message,
            path,
            line,
            column,
        })
}

/// Generate a vector of findings with controlled severity distribution.
fn arb_findings_vec() -> impl Strategy<Value = Vec<Finding>> {
    proptest::collection::vec(arb_finding(), 0..10)
}

/// Generate a vector of check reports for summary aggregation testing.
fn arb_check_reports_vec() -> impl Strategy<Value = Vec<CheckReport>> {
    proptest::collection::vec(arb_check_report(), 0..10)
}

// =============================================================================
// Property Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

    // =========================================================================
    // Property 8: Version Parsing Normalization
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 8: Version Parsing Normalization
    /// **Validates: Requirements 3.1**
    ///
    /// For any valid Rust version string, parsing should produce a normalized
    /// three-component semver version (e.g., "1.75" -> "1.75.0").
    #[test]
    fn prop_version_parsing_normalization(version_str in arb_valid_rust_version()) {
        // Parse the version string
        let parsed = parse_rust_version(&version_str);

        // The version should parse successfully
        prop_assert!(parsed.is_ok(), "Failed to parse valid version: {}", version_str);

        let version = parsed.unwrap();
        let normalized = version.to_string();

        // The normalized version should match the three-component semver pattern
        // Pattern: one or more digits, dot, one or more digits, dot, one or more digits
        let semver_pattern = regex::Regex::new(r"^\d+\.\d+\.\d+$").unwrap();
        prop_assert!(
            semver_pattern.is_match(&normalized),
            "Normalized version '{}' (from '{}') does not match semver pattern",
            normalized,
            version_str
        );
    }

    // =========================================================================
    // Property 6: Check Status Consistency
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 6: Check Status Consistency
    /// **Validates: Requirements 3.2**
    ///
    /// For any set of findings, the check status should be consistent with the
    /// highest severity finding present:
    /// - If any finding has Error severity => status == Fail
    /// - Else if any finding has Warn severity => status == Warn
    /// - Else => status == Pass
    #[test]
    fn prop_check_status_consistency(findings in arb_findings_vec()) {
        let status = check_status_from_findings(&findings);

        // Determine expected status based on highest severity
        let has_error = findings.iter().any(|f| f.severity == Severity::Error);
        let has_warn = findings.iter().any(|f| f.severity == Severity::Warn);

        let expected_status = if has_error {
            CheckStatus::Fail
        } else if has_warn {
            CheckStatus::Warn
        } else {
            CheckStatus::Pass
        };

        prop_assert_eq!(
            status,
            expected_status,
            "Status mismatch for findings: {:?}. Expected {:?} but got {:?}",
            findings.iter().map(|f| &f.severity).collect::<Vec<_>>(),
            expected_status,
            status
        );
    }

    // =========================================================================
    // Property 7: Summary Aggregation Consistency
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 7: Summary Aggregation Consistency
    /// **Validates: Requirements 3.3**
    ///
    /// For any set of check reports, the summary counts should equal the sum of
    /// individual finding counts by severity:
    /// - summary.counts.error == total Error findings across all checks
    /// - summary.counts.warn == total Warn findings across all checks
    /// - summary.counts.info == total Info findings across all checks
    #[test]
    fn prop_summary_aggregation_consistency(checks in arb_check_reports_vec()) {
        let summary = summarize(&checks);

        // Calculate expected counts by iterating through all findings
        let expected_error_count = checks
            .iter()
            .flat_map(|c| &c.findings)
            .filter(|f| f.severity == Severity::Error)
            .count();

        let expected_warn_count = checks
            .iter()
            .flat_map(|c| &c.findings)
            .filter(|f| f.severity == Severity::Warn)
            .count();

        let expected_info_count = checks
            .iter()
            .flat_map(|c| &c.findings)
            .filter(|f| f.severity == Severity::Info)
            .count();

        // Verify that summary counts match the expected counts
        prop_assert_eq!(
            summary.counts.error as usize,
            expected_error_count,
            "Error count mismatch: summary has {} but expected {} from {} checks",
            summary.counts.error,
            expected_error_count,
            checks.len()
        );

        prop_assert_eq!(
            summary.counts.warn as usize,
            expected_warn_count,
            "Warn count mismatch: summary has {} but expected {} from {} checks",
            summary.counts.warn,
            expected_warn_count,
            checks.len()
        );

        prop_assert_eq!(
            summary.counts.info as usize,
            expected_info_count,
            "Info count mismatch: summary has {} but expected {} from {} checks",
            summary.counts.info,
            expected_info_count,
            checks.len()
        );
    }
}

// =============================================================================
// Unit Tests for Generators (Sanity Checks)
// =============================================================================

#[cfg(test)]
mod generator_tests {
    use super::*;

    /// Verify that the valid version generator produces parseable versions.
    #[test]
    fn test_arb_valid_rust_version_produces_parseable() {
        // Test a few known valid versions
        assert!(parse_rust_version("1").is_ok());
        assert!(parse_rust_version("1.75").is_ok());
        assert!(parse_rust_version("1.75.0").is_ok());
    }

    /// Verify that check_status_from_findings works with empty findings.
    #[test]
    fn test_check_status_empty_findings() {
        let findings: Vec<Finding> = vec![];
        assert_eq!(check_status_from_findings(&findings), CheckStatus::Pass);
    }

    /// Verify that summarize works with empty check reports.
    #[test]
    fn test_summarize_empty_reports() {
        let reports: Vec<CheckReport> = vec![];
        let summary = summarize(&reports);
        assert_eq!(summary.verdict, Verdict::Skip);
        assert_eq!(summary.counts.info, 0);
        assert_eq!(summary.counts.warn, 0);
        assert_eq!(summary.counts.error, 0);
    }
}
