//! Golden file (snapshot) tests for builddiag-render.
//!
//! These tests use insta for snapshot testing to ensure consistent output
//! across different scenarios:
//!
//! - Empty findings
//! - Mixed severity findings
//! - Truncated output (over budget)
//!
//! Run `cargo insta review` to review and accept snapshot changes.

use builddiag_render::{
    RenderOptions, render_github_annotations_with_options, render_markdown_with_options,
};
use builddiag_types::{Finding, HostInfo, Location, Report, RunInfo, Severity, ToolInfo, Verdict};
use chrono::{TimeZone, Utc};

/// Create a fixed timestamp for deterministic tests.
fn fixed_timestamp() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 15, 10, 30, 0).unwrap()
}

/// Create a base report with common fields.
fn base_report() -> Report {
    Report {
        schema: Report::SCHEMA_V1.to_string(),
        tool: Some(ToolInfo {
            name: "builddiag".to_string(),
            version: "0.1.0".to_string(),
        }),
        run: Some(RunInfo {
            started_at: fixed_timestamp(),
            ended_at: Some(fixed_timestamp()),
            duration_ms: 150,
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            git: None,
        }),
        verdict: Verdict::Pass,
        findings: vec![],
        summary: None,
        data: None,
    }
}

// =============================================================================
// Empty Findings Tests
// =============================================================================

#[test]
fn test_golden_markdown_empty_findings() {
    let report = base_report();
    let options = RenderOptions::default();
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_empty_findings", md);
}

#[test]
fn test_golden_annotations_empty_findings() {
    let report = base_report();
    let options = RenderOptions::default();
    let annotations = render_github_annotations_with_options(&report, &options);

    insta::assert_snapshot!("annotations_empty_findings", annotations.join("\n"));
}

// =============================================================================
// Mixed Severity Findings Tests
// =============================================================================

fn report_with_mixed_findings() -> Report {
    let mut report = base_report();

    report.findings = vec![
        Finding {
            check_id: "rust.msrv_defined".to_string(),
            code: "missing_msrv".to_string(),
            severity: Severity::Error,
            message: "Missing rust-version field in Cargo.toml".to_string(),
            location: Some(Location {
                path: "Cargo.toml".to_string(),
                line: Some(1),
                col: None,
            }),
        },
        Finding {
            check_id: "rust.toolchain_pinning".to_string(),
            code: "unpinned_toolchain".to_string(),
            severity: Severity::Warn,
            message: "Toolchain is not pinned to a specific version".to_string(),
            location: Some(Location {
                path: "rust-toolchain.toml".to_string(),
                line: Some(2),
                col: Some(10),
            }),
        },
        Finding {
            check_id: "rust.toolchain_pinning".to_string(),
            code: "toolchain_channel".to_string(),
            severity: Severity::Info,
            message: "Detected toolchain channel: stable".to_string(),
            location: Some(Location {
                path: "rust-toolchain.toml".to_string(),
                line: Some(1),
                col: None,
            }),
        },
        Finding {
            check_id: "workspace.resolver_v2".to_string(),
            code: "resolver_v1".to_string(),
            severity: Severity::Warn,
            message: "Workspace uses resolver version 1, consider upgrading to v2".to_string(),
            location: Some(Location {
                path: "Cargo.toml".to_string(),
                line: Some(5),
                col: None,
            }),
        },
    ];

    report.verdict = Verdict::Fail;
    report
}

#[test]
fn test_golden_markdown_mixed_findings() {
    let report = report_with_mixed_findings();
    let options = RenderOptions {
        max_findings: 50,
        show_info: false, // Default behavior
    };
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_mixed_findings", md);
}

#[test]
fn test_golden_markdown_mixed_findings_with_info() {
    let report = report_with_mixed_findings();
    let options = RenderOptions {
        max_findings: 50,
        show_info: true,
    };
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_mixed_findings_with_info", md);
}

#[test]
fn test_golden_annotations_mixed_findings() {
    let report = report_with_mixed_findings();
    let options = RenderOptions {
        max_findings: 50,
        show_info: false,
    };
    let annotations = render_github_annotations_with_options(&report, &options);

    insta::assert_snapshot!("annotations_mixed_findings", annotations.join("\n"));
}

#[test]
fn test_golden_annotations_mixed_findings_with_info() {
    let report = report_with_mixed_findings();
    let options = RenderOptions {
        max_findings: 50,
        show_info: true,
    };
    let annotations = render_github_annotations_with_options(&report, &options);

    insta::assert_snapshot!(
        "annotations_mixed_findings_with_info",
        annotations.join("\n")
    );
}

// =============================================================================
// Truncated Output Tests
// =============================================================================

fn report_with_many_findings() -> Report {
    let mut report = base_report();

    let mut findings = Vec::new();
    for i in 0..20 {
        findings.push(Finding {
            check_id: "test.bulk_check".to_string(),
            code: format!("finding_{:02}", i),
            severity: if i < 5 {
                Severity::Error
            } else if i < 12 {
                Severity::Warn
            } else {
                Severity::Info
            },
            message: format!("Finding number {} with detailed description", i),
            location: Some(Location {
                path: format!("src/module_{}.rs", i % 5),
                line: Some((i * 10 + 1) as u32),
                col: if i % 2 == 0 {
                    Some((i + 1) as u32)
                } else {
                    None
                },
            }),
        });
    }

    report.findings = findings;
    report.verdict = Verdict::Fail;
    report
}

#[test]
fn test_golden_markdown_truncated() {
    let report = report_with_many_findings();
    let options = RenderOptions {
        max_findings: 5,
        show_info: false,
    };
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_truncated", md);
}

#[test]
fn test_golden_markdown_truncated_with_info() {
    let report = report_with_many_findings();
    let options = RenderOptions {
        max_findings: 10,
        show_info: true,
    };
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_truncated_with_info", md);
}

#[test]
fn test_golden_annotations_truncated() {
    let report = report_with_many_findings();
    let options = RenderOptions {
        max_findings: 5,
        show_info: true,
    };
    let annotations = render_github_annotations_with_options(&report, &options);

    insta::assert_snapshot!("annotations_truncated", annotations.join("\n"));
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
fn test_golden_markdown_special_characters() {
    let mut report = base_report();

    report.findings = vec![
        Finding {
            check_id: "test.special_chars".to_string(),
            code: "pipe|in|code".to_string(),
            severity: Severity::Error,
            message: "Message with | pipe and \\ backslash".to_string(),
            location: Some(Location {
                path: "path|with|pipes.rs".to_string(),
                line: Some(1),
                col: None,
            }),
        },
        Finding {
            check_id: "test.special_chars".to_string(),
            code: "newline_test".to_string(),
            severity: Severity::Warn,
            message: "Message with\nnewline character".to_string(),
            location: Some(Location {
                path: "file.rs".to_string(),
                line: Some(10),
                col: None,
            }),
        },
    ];

    report.verdict = Verdict::Fail;

    let options = RenderOptions::default();
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_special_characters", md);
}

#[test]
fn test_golden_annotations_special_characters() {
    let mut report = base_report();

    report.findings = vec![Finding {
        check_id: "test.special_chars".to_string(),
        code: "percent_test".to_string(),
        severity: Severity::Error,
        message: "100% complete with\nnewline".to_string(),
        location: Some(Location {
            path: "file.rs".to_string(),
            line: Some(1),
            col: None,
        }),
    }];

    let options = RenderOptions::default();
    let annotations = render_github_annotations_with_options(&report, &options);

    insta::assert_snapshot!("annotations_special_characters", annotations.join("\n"));
}

#[test]
fn test_golden_markdown_no_location() {
    let mut report = base_report();

    report.findings = vec![
        Finding {
            check_id: "test.no_location".to_string(),
            code: "global_error".to_string(),
            severity: Severity::Error,
            message: "A finding without any location information".to_string(),
            location: None,
        },
        Finding {
            check_id: "test.no_location".to_string(),
            code: "path_only".to_string(),
            severity: Severity::Warn,
            message: "A finding with only path, no line".to_string(),
            location: Some(Location {
                path: "some/path.rs".to_string(),
                line: None,
                col: None,
            }),
        },
    ];

    report.verdict = Verdict::Fail;

    let options = RenderOptions::default();
    let md = render_markdown_with_options(&report, &options);

    insta::assert_snapshot!("markdown_no_location", md);
}

#[test]
fn test_golden_markdown_all_verdicts() {
    let options = RenderOptions::default();

    // Pass verdict
    let mut pass_report = base_report();
    pass_report.verdict = Verdict::Pass;
    let pass_md = render_markdown_with_options(&pass_report, &options);
    insta::assert_snapshot!("markdown_verdict_pass", pass_md);

    // Warn verdict
    let mut warn_report = base_report();
    warn_report.verdict = Verdict::Warn;
    let warn_md = render_markdown_with_options(&warn_report, &options);
    insta::assert_snapshot!("markdown_verdict_warn", warn_md);

    // Fail verdict
    let mut fail_report = base_report();
    fail_report.verdict = Verdict::Fail;
    let fail_md = render_markdown_with_options(&fail_report, &options);
    insta::assert_snapshot!("markdown_verdict_fail", fail_md);

    // Skip verdict
    let mut skip_report = base_report();
    skip_report.verdict = Verdict::Skip;
    let skip_md = render_markdown_with_options(&skip_report, &options);
    insta::assert_snapshot!("markdown_verdict_skip", skip_md);

    // Error verdict
    let mut error_report = base_report();
    error_report.verdict = Verdict::Error;
    let error_md = render_markdown_with_options(&error_report, &options);
    insta::assert_snapshot!("markdown_verdict_error", error_md);
}
