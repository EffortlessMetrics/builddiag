//! Property-based tests for builddiag-render.
//!
//! This module contains property tests that validate universal invariants
//! for the rendering functions in builddiag-render, focusing on:
//!
//! - Deterministic output ordering (Property 5)
//! - Markdown rendering consistency
//! - GitHub annotation formatting
//!
//! # Properties Tested
//!
//! - **Property 5**: Deterministic Output Ordering (Requirements 8.8)
//! - Markdown output consistency (Requirements 3.5)
//! - GitHub annotation formatting (Requirements 3.6)

use builddiag_render::{render_github_annotations, render_markdown};
use builddiag_types::{
    CheckReport, CheckStatus, Finding, Inputs, RepoDetected, RepoInfo, Report, RunInfo, SchemaId,
    Severity, Summary, SummaryCounts, ToolInfo, Verdict,
};
use chrono::{TimeZone, Utc};
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
/// Avoids pipe characters to simplify markdown table testing.
fn arb_message() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?-]{1,50}".prop_map(|s| s.to_string())
}

/// Generate arbitrary file paths.
fn arb_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/]{0,20}\\.(toml|rs|md)".prop_map(|s| s.to_string())
}

/// Generate arbitrary version strings.
fn arb_version() -> impl Strategy<Value = String> {
    (0u32..10, 0u32..100, 0u32..100).prop_map(|(maj, min, pat)| format!("{}.{}.{}", maj, min, pat))
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

/// Generate arbitrary Verdict values.
fn arb_verdict() -> impl Strategy<Value = Verdict> {
    prop_oneof![
        Just(Verdict::Pass),
        Just(Verdict::Warn),
        Just(Verdict::Fail),
        Just(Verdict::Skip),
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

/// Generate arbitrary Finding instances with path and line (for GitHub annotations).
#[allow(dead_code)] // Will be used in subsequent tasks (5.2, 5.3)
fn arb_finding_with_location() -> impl Strategy<Value = Finding> {
    (
        arb_severity(),
        arb_identifier(),
        arb_message(),
        arb_path(),
        1u32..1000,
        proptest::option::of(1u32..200),
    )
        .prop_map(|(severity, code, message, path, line, column)| Finding {
            severity,
            code,
            message,
            path: Some(path),
            line: Some(line),
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

/// Generate CheckReport instances that will produce findings in markdown output.
/// These have Warn or Fail status with at least one finding.
#[allow(dead_code)] // Will be used in subsequent tasks (5.2, 5.3)
fn arb_check_report_with_findings() -> impl Strategy<Value = CheckReport> {
    (
        arb_identifier(),
        prop_oneof![Just(CheckStatus::Warn), Just(CheckStatus::Fail)],
        proptest::collection::vec(arb_finding(), 1..5),
    )
        .prop_map(|(id, status, findings)| CheckReport {
            id,
            status,
            findings,
            skipped_reason: None,
        })
}

/// Generate arbitrary SummaryCounts instances.
fn arb_summary_counts() -> impl Strategy<Value = SummaryCounts> {
    (0usize..100, 0usize..100, 0usize..100).prop_map(|(info, warn, error)| SummaryCounts {
        info,
        warn,
        error,
    })
}

/// Generate arbitrary Summary instances.
fn arb_summary() -> impl Strategy<Value = Summary> {
    (
        arb_summary_counts(),
        arb_verdict(),
        proptest::collection::vec(arb_message(), 0..5),
    )
        .prop_map(|(counts, verdict, reasons)| Summary {
            counts,
            verdict,
            reasons,
        })
}

/// Generate arbitrary ToolInfo instances.
fn arb_tool_info() -> impl Strategy<Value = ToolInfo> {
    (arb_identifier(), arb_version()).prop_map(|(name, version)| ToolInfo { name, version })
}

/// Generate a valid UTC timestamp within a reasonable range.
fn arb_datetime() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    // Generate timestamps between 2020 and 2030
    (
        2020i32..2030,
        1u32..13,
        1u32..29,
        0u32..24,
        0u32..60,
        0u32..60,
    )
        .prop_map(|(year, month, day, hour, min, sec)| {
            Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
                .single()
                .unwrap_or_else(|| Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap())
        })
}

/// Generate arbitrary RunInfo instances.
fn arb_run_info() -> impl Strategy<Value = RunInfo> {
    (
        arb_identifier(),
        arb_datetime(),
        proptest::option::of(arb_datetime()),
    )
        .prop_map(|(id, started_at, ended_at)| RunInfo {
            id,
            started_at,
            ended_at,
        })
}

/// Generate arbitrary RepoDetected instances.
fn arb_repo_detected() -> impl Strategy<Value = RepoDetected> {
    (any::<bool>(), 1usize..20).prop_map(|(is_workspace, members)| RepoDetected {
        is_workspace,
        members,
    })
}

/// Generate arbitrary RepoInfo instances.
fn arb_repo_info() -> impl Strategy<Value = RepoInfo> {
    (arb_path(), arb_repo_detected()).prop_map(|(root, detected)| RepoInfo { root, detected })
}

/// Generate arbitrary Inputs instances.
fn arb_inputs() -> impl Strategy<Value = Inputs> {
    (
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
    )
        .prop_map(
            |(cargo_root, rust_toolchain, tools_checksums, tools_manifest)| Inputs {
                cargo_root,
                rust_toolchain,
                tools_checksums,
                tools_manifest,
            },
        )
}

/// Generate arbitrary SchemaId instances.
fn arb_schema_id() -> impl Strategy<Value = SchemaId> {
    arb_identifier().prop_map(SchemaId)
}

/// Generate arbitrary Report instances.
fn arb_report() -> impl Strategy<Value = Report> {
    (
        arb_schema_id(),
        arb_tool_info(),
        arb_run_info(),
        arb_repo_info(),
        arb_inputs(),
        proptest::collection::vec(arb_check_report(), 0..10),
        arb_summary(),
    )
        .prop_map(
            |(schema, tool, run, repo, inputs, checks, summary)| Report {
                schema,
                tool,
                run,
                repo,
                inputs,
                checks,
                summary,
            },
        )
}

/// Generate Report instances that will produce non-empty markdown output.
/// These have at least one check with Warn or Fail status and findings.
#[allow(dead_code)] // Will be used in subsequent tasks (5.2, 5.3)
fn arb_report_with_findings() -> impl Strategy<Value = Report> {
    (
        arb_schema_id(),
        arb_tool_info(),
        arb_run_info(),
        arb_repo_info(),
        arb_inputs(),
        proptest::collection::vec(arb_check_report_with_findings(), 1..5),
        arb_summary(),
    )
        .prop_map(
            |(schema, tool, run, repo, inputs, checks, summary)| Report {
                schema,
                tool,
                run,
                repo,
                inputs,
                checks,
                summary,
            },
        )
}

// =============================================================================
// Property Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

    // =========================================================================
    // Basic Rendering Properties
    // =========================================================================

    /// Verify that render_markdown never panics for any valid Report.
    ///
    /// This is a basic sanity check that the rendering function handles
    /// all possible Report configurations without crashing.
    #[test]
    fn prop_render_markdown_never_panics(report in arb_report()) {
        // Should not panic
        let _md = render_markdown(&report);
    }

    /// Verify that render_github_annotations never panics for any valid Report.
    ///
    /// This is a basic sanity check that the annotation rendering function
    /// handles all possible Report configurations without crashing.
    #[test]
    fn prop_render_annotations_never_panics(report in arb_report()) {
        // Should not panic
        let _annotations = render_github_annotations(&report);
    }

    /// Verify that markdown output always contains the header section.
    ///
    /// The markdown output should always start with the builddiag header
    /// containing the verdict icon and summary counts.
    #[test]
    fn prop_markdown_contains_header(report in arb_report()) {
        let md = render_markdown(&report);
        prop_assert!(md.starts_with("## builddiag:"));
        prop_assert!(md.contains("errors"));
        prop_assert!(md.contains("warnings"));
    }

    /// Verify that markdown output contains reproduce section when there are findings.
    ///
    /// When there are findings to display, the markdown output should include
    /// instructions on how to reproduce the check locally.
    /// When there are no findings, it shows "No findings." instead.
    #[test]
    fn prop_markdown_structure_consistent(report in arb_report()) {
        let md = render_markdown(&report);

        // Check if there are any findings that would be displayed
        let has_displayable_findings = report.checks.iter().any(|c| {
            (c.status == CheckStatus::Warn || c.status == CheckStatus::Fail) && !c.findings.is_empty()
        });

        if has_displayable_findings {
            // When there are findings, should have table and reproduce section
            prop_assert!(md.contains("| severity |"));
            prop_assert!(md.contains("Reproduce:"));
            prop_assert!(md.contains("builddiag check"));
        } else {
            // When no findings, should show "No findings."
            prop_assert!(md.contains("No findings."));
        }
    }

    // =========================================================================
    // Property 5: Deterministic Output Ordering
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 5: Deterministic Output Ordering
    /// **Validates: Requirements 8.8**
    ///
    /// For any input, running the tool twice should produce byte-identical output.
    /// This validates that BTreeMap/BTreeSet are used correctly and output is deterministic.
    #[test]
    fn prop_deterministic_output_ordering(report in arb_report()) {
        // Run render_markdown twice with the same input
        let md1 = render_markdown(&report);
        let md2 = render_markdown(&report);

        // Output should be byte-identical
        prop_assert_eq!(
            md1, md2,
            "render_markdown should produce identical output for the same input"
        );

        // Run render_github_annotations twice with the same input
        let annotations1 = render_github_annotations(&report);
        let annotations2 = render_github_annotations(&report);

        // Output should be byte-identical
        prop_assert_eq!(
            annotations1, annotations2,
            "render_github_annotations should produce identical output for the same input"
        );
    }

    // =========================================================================
    // Markdown Consistency Property
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Markdown Consistency
    /// **Validates: Requirements 3.5**
    ///
    /// Verify that markdown output contains expected sections for any report:
    /// - Header with verdict icon
    /// - Summary counts (errors, warnings)
    /// - Findings table when there are findings
    /// - Reproduce section when there are findings
    #[test]
    fn prop_markdown_consistency(report in arb_report()) {
        let md = render_markdown(&report);

        // 1. Header with verdict icon - should always be present
        prop_assert!(
            md.starts_with("## builddiag:"),
            "Markdown should start with builddiag header"
        );

        // Verify verdict icon is present (one of ✅, ⚠️, ❌, ⏭️)
        let has_verdict_icon = md.contains("✅") || md.contains("⚠️") || md.contains("❌") || md.contains("⏭️");
        prop_assert!(
            has_verdict_icon,
            "Markdown header should contain a verdict icon"
        );

        // 2. Summary counts - should always be present in header
        prop_assert!(
            md.contains("errors"),
            "Markdown should contain error count"
        );
        prop_assert!(
            md.contains("warnings"),
            "Markdown should contain warning count"
        );

        // Verify the counts in the header match the report summary
        let expected_error_count = format!("{} errors", report.summary.counts.error);
        let expected_warn_count = format!("{} warnings", report.summary.counts.warn);
        prop_assert!(
            md.contains(&expected_error_count),
            "Markdown should contain correct error count: expected '{}' in output",
            expected_error_count
        );
        prop_assert!(
            md.contains(&expected_warn_count),
            "Markdown should contain correct warning count: expected '{}' in output",
            expected_warn_count
        );

        // Check if there are any findings that would be displayed
        // (only Warn or Fail status checks with non-empty findings are shown)
        let has_displayable_findings = report.checks.iter().any(|c| {
            (c.status == CheckStatus::Warn || c.status == CheckStatus::Fail) && !c.findings.is_empty()
        });

        if has_displayable_findings {
            // 3. Findings table - should be present when there are findings
            prop_assert!(
                md.contains("| severity |"),
                "Markdown should contain findings table header when there are findings"
            );
            prop_assert!(
                md.contains("| check |"),
                "Markdown should contain check column in findings table"
            );
            prop_assert!(
                md.contains("| location |"),
                "Markdown should contain location column in findings table"
            );
            prop_assert!(
                md.contains("| message |"),
                "Markdown should contain message column in findings table"
            );
            prop_assert!(
                md.contains("|---|---|---|---|"),
                "Markdown should contain table separator row"
            );

            // 4. Reproduce section - should be present when there are findings
            prop_assert!(
                md.contains("Reproduce:"),
                "Markdown should contain Reproduce section when there are findings"
            );
            prop_assert!(
                md.contains("builddiag check"),
                "Markdown should contain builddiag check command in Reproduce section"
            );
        } else {
            // When no findings, should show "No findings." message
            prop_assert!(
                md.contains("No findings."),
                "Markdown should contain 'No findings.' when there are no displayable findings"
            );
        }
    }

    /// Feature: comprehensive-test-coverage, Markdown Consistency with Findings
    /// **Validates: Requirements 3.5**
    ///
    /// Verify that markdown output for reports with findings always contains
    /// the expected table structure and reproduce section.
    #[test]
    fn prop_markdown_consistency_with_findings(report in arb_report_with_findings()) {
        let md = render_markdown(&report);

        // Header should always be present
        prop_assert!(
            md.starts_with("## builddiag:"),
            "Markdown should start with builddiag header"
        );

        // For reports with findings, table should always be present
        prop_assert!(
            md.contains("| severity | check | location | message |"),
            "Markdown should contain complete table header for reports with findings"
        );

        // Reproduce section should always be present for reports with findings
        prop_assert!(
            md.contains("Reproduce:"),
            "Markdown should contain Reproduce section for reports with findings"
        );
        prop_assert!(
            md.contains("`builddiag check --root .`"),
            "Markdown should contain exact reproduce command"
        );

        // Should NOT contain "No findings." when there are findings
        prop_assert!(
            !md.contains("No findings."),
            "Markdown should not contain 'No findings.' when there are findings"
        );
    }
}
