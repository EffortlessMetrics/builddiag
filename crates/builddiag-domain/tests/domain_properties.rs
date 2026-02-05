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

use builddiag_domain::{
    check_status_from_findings, parse_rust_version, sort_findings_canonical, summarize,
};
use builddiag_types::{CheckReport, CheckStatus, Finding, Location, Severity};
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

/// Generate arbitrary Location instances.
fn arb_location() -> impl Strategy<Value = Location> {
    (
        arb_path(),
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(|(path, line, col)| Location { path, line, col })
}

/// Generate arbitrary Finding instances.
fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_identifier(),
        arb_identifier(),
        arb_severity(),
        arb_message(),
        proptest::option::of(arb_location()),
    )
        .prop_map(|(check_id, code, severity, message, location)| Finding {
            check_id,
            code,
            severity,
            message,
            location,
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
        arb_identifier(),
        arb_message(),
        proptest::option::of(arb_location()),
    )
        .prop_map(move |(check_id, code, message, location)| Finding {
            check_id,
            code,
            severity,
            message,
            location,
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
        let actual_error_count = *summary.by_severity.get("error").unwrap_or(&0);
        let actual_warn_count = *summary.by_severity.get("warn").unwrap_or(&0);
        let actual_info_count = *summary.by_severity.get("info").unwrap_or(&0);

        prop_assert_eq!(
            actual_error_count,
            expected_error_count,
            "Error count mismatch: summary has {} but expected {} from {} checks",
            actual_error_count,
            expected_error_count,
            checks.len()
        );

        prop_assert_eq!(
            actual_warn_count,
            expected_warn_count,
            "Warn count mismatch: summary has {} but expected {} from {} checks",
            actual_warn_count,
            expected_warn_count,
            checks.len()
        );

        prop_assert_eq!(
            actual_info_count,
            expected_info_count,
            "Info count mismatch: summary has {} but expected {} from {} checks",
            actual_info_count,
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
        assert_eq!(summary.total_findings, 0);
        assert_eq!(*summary.by_severity.get("info").unwrap_or(&0), 0);
        assert_eq!(*summary.by_severity.get("warn").unwrap_or(&0), 0);
        assert_eq!(*summary.by_severity.get("error").unwrap_or(&0), 0);
    }
}

// =============================================================================
// Property Tests for Canonical Sorting
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

    // =========================================================================
    // Property: Sorting Idempotency
    // =========================================================================

    /// Feature: deterministic-output, Property: Sorting Idempotency
    ///
    /// For any vector of findings, sorting twice produces the same result as
    /// sorting once. This ensures that the sort operation is stable and does
    /// not reorder equal elements differently on subsequent calls.
    #[test]
    fn prop_sort_findings_idempotent(findings in arb_findings_vec()) {
        let mut first_sort = findings.clone();
        sort_findings_canonical(&mut first_sort);

        let mut second_sort = first_sort.clone();
        sort_findings_canonical(&mut second_sort);

        prop_assert_eq!(
            first_sort,
            second_sort,
            "Sorting should be idempotent: sorting twice should give same result as sorting once"
        );
    }

    // =========================================================================
    // Property: Sorting Determinism
    // =========================================================================

    /// Feature: deterministic-output, Property: Sorting Determinism
    ///
    /// For any vector of findings, sorting produces the same result regardless
    /// of the initial order. This is tested by comparing sorting of the original
    /// vector with sorting of a shuffled version.
    #[test]
    fn prop_sort_findings_deterministic(
        findings in arb_findings_vec(),
        seed in any::<u64>()
    ) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Create two copies with potentially different orderings
        let mut sorted1 = findings.clone();
        sort_findings_canonical(&mut sorted1);

        // Create a "shuffled" version by sorting by hash
        let mut shuffled = findings.clone();
        shuffled.sort_by(|a, b| {
            let mut ha = DefaultHasher::new();
            let mut hb = DefaultHasher::new();
            // Hash with seed for different orderings
            seed.hash(&mut ha);
            a.code.hash(&mut ha);
            a.message.hash(&mut ha);
            seed.hash(&mut hb);
            b.code.hash(&mut hb);
            b.message.hash(&mut hb);
            ha.finish().cmp(&hb.finish())
        });

        let mut sorted2 = shuffled;
        sort_findings_canonical(&mut sorted2);

        prop_assert_eq!(
            sorted1,
            sorted2,
            "Sorting should be deterministic: same elements in different order should produce same sorted result"
        );
    }

    // =========================================================================
    // Property: Sorting Totality (No Panics)
    // =========================================================================

    /// Feature: deterministic-output, Property: Sorting Totality
    ///
    /// For any valid vector of findings, sorting should complete without
    /// panicking and produce a valid result. This tests that the comparison
    /// function handles all edge cases (None values, empty strings, etc.).
    #[test]
    fn prop_sort_findings_total(findings in arb_findings_vec()) {
        let mut to_sort = findings.clone();

        // Should not panic
        sort_findings_canonical(&mut to_sort);

        // Result should have same length
        prop_assert_eq!(
            to_sort.len(),
            findings.len(),
            "Sorting should preserve all elements"
        );

        // Result should contain all original elements (unordered comparison)
        for f in &findings {
            prop_assert!(
                to_sort.contains(f),
                "Sorted result should contain all original findings"
            );
        }
    }

    // =========================================================================
    // Property: Severity Ordering
    // =========================================================================

    /// Feature: deterministic-output, Property: Severity Ordering
    ///
    /// After sorting, findings should be ordered by severity (Error > Warn > Info).
    /// This means errors come first, then warnings, then info.
    #[test]
    fn prop_sort_findings_severity_order(findings in arb_findings_vec()) {
        let mut sorted = findings.clone();
        sort_findings_canonical(&mut sorted);

        // Check that severity is non-increasing (Error >= Warn >= Info)
        for window in sorted.windows(2) {
            let prev_severity = &window[0].severity;
            let curr_severity = &window[1].severity;

            // In our ordering: Error > Warn > Info
            // So prev should be >= curr (where Error is greatest)
            prop_assert!(
                prev_severity >= curr_severity,
                "Findings should be sorted by severity (Error > Warn > Info), but found {:?} before {:?}",
                prev_severity,
                curr_severity
            );
        }
    }
}
