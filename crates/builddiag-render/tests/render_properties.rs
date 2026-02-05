//! Property-based tests for builddiag-render.
//!
//! This module contains property tests that validate universal invariants
//! for the rendering functions in builddiag-render, focusing on:
//!
//! - Deterministic output ordering (Property 5)
//! - Markdown rendering consistency
//! - GitHub annotation formatting
//! - Budget-aware truncation
//!
//! # Properties Tested
//!
//! - **Property 5**: Deterministic Output Ordering (Requirements 8.8)
//! - Markdown output consistency (Requirements 3.5)
//! - GitHub annotation formatting (Requirements 3.6)
//! - Budget-aware rendering with truncation

use builddiag_render::{
    RenderOptions, render_github_annotations, render_github_annotations_with_options,
    render_markdown, render_markdown_with_options,
};
use builddiag_types::{Finding, HostInfo, Location, Report, RunInfo, Severity, ToolInfo, Verdict};
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

/// Generate arbitrary Verdict values.
fn arb_verdict() -> impl Strategy<Value = Verdict> {
    prop_oneof![
        Just(Verdict::Pass),
        Just(Verdict::Warn),
        Just(Verdict::Fail),
        Just(Verdict::Skip),
        Just(Verdict::Error),
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
        arb_identifier(), // check_id
        arb_identifier(), // code
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

/// Generate arbitrary Finding instances with path and line (for GitHub annotations).
fn arb_finding_with_location() -> impl Strategy<Value = Finding> {
    (
        arb_identifier(), // check_id
        arb_identifier(), // code
        arb_severity(),
        arb_message(),
        arb_path(),
        1u32..1000,
        proptest::option::of(1u32..200),
    )
        .prop_map(
            |(check_id, code, severity, message, path, line, col)| Finding {
                check_id,
                code,
                severity,
                message,
                location: Some(Location {
                    path,
                    line: Some(line),
                    col,
                }),
            },
        )
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

/// Generate arbitrary HostInfo instances.
fn arb_host_info() -> impl Strategy<Value = HostInfo> {
    (
        prop_oneof![Just("linux"), Just("macos"), Just("windows")],
        prop_oneof![Just("x86_64"), Just("aarch64")],
    )
        .prop_map(|(os, arch)| HostInfo {
            os: os.to_string(),
            arch: arch.to_string(),
        })
}

/// Generate arbitrary RunInfo instances.
fn arb_run_info() -> impl Strategy<Value = RunInfo> {
    (
        arb_datetime(),
        proptest::option::of(arb_datetime()),
        0u64..10000,
        arb_host_info(),
    )
        .prop_map(|(started_at, ended_at, duration_ms, host)| RunInfo {
            started_at,
            ended_at,
            duration_ms,
            host,
            git: None,
        })
}

/// Generate arbitrary Report instances.
fn arb_report() -> impl Strategy<Value = Report> {
    (
        proptest::option::of(arb_tool_info()),
        proptest::option::of(arb_run_info()),
        arb_verdict(),
        proptest::collection::vec(arb_finding(), 0..20),
    )
        .prop_map(|(tool, run, verdict, findings)| Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool,
            run,
            verdict,
            findings,
            summary: None,
            data: None,
        })
}

/// Generate Report instances that will produce non-empty markdown output.
/// These have at least one non-info finding.
fn arb_report_with_findings() -> impl Strategy<Value = Report> {
    (
        proptest::option::of(arb_tool_info()),
        proptest::option::of(arb_run_info()),
        arb_verdict(),
        proptest::collection::vec(arb_finding(), 1..10),
    )
        .prop_map(|(tool, run, verdict, findings)| Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool,
            run,
            verdict,
            findings,
            summary: None,
            data: None,
        })
}

/// Generate Report instances that will produce GitHub annotations.
/// These have findings with path and line information.
fn arb_report_with_located_findings() -> impl Strategy<Value = Report> {
    (
        proptest::option::of(arb_tool_info()),
        proptest::option::of(arb_run_info()),
        arb_verdict(),
        proptest::collection::vec(arb_finding_with_location(), 1..10),
    )
        .prop_map(|(tool, run, verdict, findings)| Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool,
            run,
            verdict,
            findings,
            summary: None,
            data: None,
        })
}

/// Generate arbitrary RenderOptions.
fn arb_render_options() -> impl Strategy<Value = RenderOptions> {
    (1usize..100, any::<bool>()).prop_map(|(max_findings, show_info)| RenderOptions {
        max_findings,
        show_info,
    })
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
        // (non-info findings since show_info defaults to false)
        let has_displayable_findings = report.findings.iter().any(|f| f.severity != Severity::Info);

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

    /// Verify deterministic output with custom options.
    #[test]
    fn prop_deterministic_output_with_options(
        report in arb_report(),
        options in arb_render_options()
    ) {
        let md1 = render_markdown_with_options(&report, &options);
        let md2 = render_markdown_with_options(&report, &options);
        prop_assert_eq!(md1, md2);

        let ann1 = render_github_annotations_with_options(&report, &options);
        let ann2 = render_github_annotations_with_options(&report, &options);
        prop_assert_eq!(ann1, ann2);
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
    #[test]
    fn prop_markdown_consistency(report in arb_report()) {
        let md = render_markdown(&report);

        // 1. Header with verdict icon - should always be present
        prop_assert!(
            md.starts_with("## builddiag:"),
            "Markdown should start with builddiag header"
        );

        // Verify verdict icon is present (one of the expected icons)
        let has_verdict_icon = md.contains('\u{2705}') || md.contains('\u{26A0}')
            || md.contains('\u{274C}') || md.contains('\u{23ED}') || md.contains('\u{1F4A5}');
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
    }

    /// Feature: comprehensive-test-coverage, Markdown Consistency with Findings
    /// **Validates: Requirements 3.5**
    ///
    /// Verify that markdown output for reports with findings always contains
    /// the expected table structure and reproduce section.
    #[test]
    fn prop_markdown_consistency_with_findings(report in arb_report_with_findings()) {
        // Use show_info: true to ensure we see all findings
        let options = RenderOptions {
            max_findings: 1000,
            show_info: true,
        };
        let md = render_markdown_with_options(&report, &options);

        // Header should always be present
        prop_assert!(
            md.starts_with("## builddiag:"),
            "Markdown should start with builddiag header"
        );

        // For reports with findings, table should always be present
        prop_assert!(
            md.contains("| severity | check | code | location | message |"),
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

    // =========================================================================
    // Budget-Aware Rendering Properties
    // =========================================================================

    /// Verify that markdown output respects max_findings budget.
    #[test]
    fn prop_markdown_respects_budget(report in arb_report_with_findings()) {
        let options = RenderOptions {
            max_findings: 3,
            show_info: true,
        };
        let md = render_markdown_with_options(&report, &options);

        // Count table rows (data rows start with "| error", "| warn", or "| info")
        let row_count = md.lines()
            .filter(|l| l.starts_with("| error") || l.starts_with("| warn") || l.starts_with("| info"))
            .count();

        prop_assert!(
            row_count <= 3,
            "Markdown should have at most max_findings rows, got {}",
            row_count
        );
    }

    /// Verify that GitHub annotations respect max_findings budget.
    #[test]
    fn prop_annotations_respect_budget(report in arb_report_with_located_findings()) {
        let options = RenderOptions {
            max_findings: 5,
            show_info: true,
        };
        let annotations = render_github_annotations_with_options(&report, &options);

        prop_assert!(
            annotations.len() <= 5,
            "Should have at most max_findings annotations, got {}",
            annotations.len()
        );
    }

    /// Verify that show_info=false filters out info-level findings.
    #[test]
    fn prop_show_info_filters_correctly(report in arb_report_with_findings()) {
        let options_no_info = RenderOptions {
            max_findings: 1000,
            show_info: false,
        };

        let md_no_info = render_markdown_with_options(&report, &options_no_info);

        // With show_info=false, should not contain "| info |"
        prop_assert!(
            !md_no_info.contains("| info |"),
            "Markdown with show_info=false should not contain info rows"
        );
    }

    // =========================================================================
    // GitHub Annotation Formatting Properties
    // =========================================================================

    /// Verify that all GitHub annotations have valid format.
    #[test]
    fn prop_github_annotations_format(report in arb_report_with_located_findings()) {
        let options = RenderOptions {
            max_findings: 100,
            show_info: true,
        };
        let annotations = render_github_annotations_with_options(&report, &options);

        for ann in &annotations {
            // Each annotation should start with ::error, ::warning, or ::notice
            prop_assert!(
                ann.starts_with("::error") || ann.starts_with("::warning") || ann.starts_with("::notice"),
                "Annotation should start with valid severity prefix: {}",
                ann
            );

            // Should contain file parameter
            prop_assert!(
                ann.contains("file="),
                "Annotation should contain file parameter: {}",
                ann
            );

            // Should contain line parameter
            prop_assert!(
                ann.contains("line="),
                "Annotation should contain line parameter: {}",
                ann
            );

            // Should contain the delimiter "::" followed by message
            let parts: Vec<&str> = ann.splitn(2, "::").collect();
            prop_assert!(
                parts.len() == 2,
                "Annotation should have correct format with :: delimiter"
            );
        }
    }

    /// Verify severity ordering in output (errors first, then warnings, then info).
    #[test]
    fn prop_findings_sorted_by_severity(report in arb_report_with_findings()) {
        let options = RenderOptions {
            max_findings: 1000,
            show_info: true,
        };
        let md = render_markdown_with_options(&report, &options);

        // Extract severity column from each row
        let severities: Vec<&str> = md.lines()
            .filter(|l| l.starts_with("| error") || l.starts_with("| warn") || l.starts_with("| info"))
            .filter_map(|l| l.split('|').nth(1))
            .map(|s| s.trim())
            .collect();

        // Verify ordering: all errors should come before warnings, all warnings before info
        let mut seen_warn = false;
        let mut seen_info = false;
        for sev in &severities {
            match *sev {
                "error" => {
                    prop_assert!(!seen_warn && !seen_info, "Errors should come before warnings and info");
                }
                "warn" => {
                    prop_assert!(!seen_info, "Warnings should come before info");
                    seen_warn = true;
                }
                "info" => {
                    seen_info = true;
                }
                _ => {}
            }
        }
    }
}
