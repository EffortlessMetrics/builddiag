//! Rendering module for builddiag reports.
//!
//! This module provides functions to render [`Report`] data into various output formats:
//!
//! - **Markdown** - Human-readable summary suitable for PR comments
//! - **GitHub Annotations** - Inline annotations for GitHub Actions
//!
//! # Budget-Aware Rendering
//!
//! Both renderers support budget-awareness to handle large reports gracefully:
//!
//! - `max_findings` - Maximum number of findings to display (default: 50)
//! - `show_info` - Whether to include info-level findings (default: false)
//!
//! When the number of findings exceeds the budget, a truncation note is added.
//!
//! # Deterministic Output
//!
//! All output is deterministic - findings are sorted by severity (errors first),
//! then by check ID, path, and line number. This ensures reproducible output
//! for the same input.
//!
//! # Example
//!
//! ```
//! use builddiag_render::{render_markdown, render_markdown_with_options, render_github_annotations, render_github_annotations_with_options, RenderOptions};
//! # use builddiag_types::*;
//! # use chrono::Utc;
//!
//! # let report = Report {
//! #     schema: Report::SCHEMA_V1.to_string(),
//! #     tool: ToolInfo { name: "builddiag".into(), version: "0.1.0".into() },
//! #     run: RunInfo {
//! #         started_at: Utc::now(),
//! #         ended_at: None,
//! #         duration_ms: 100,
//! #         host: HostInfo { os: "linux".into(), arch: "x86_64".into() },
//! #         git: None,
//! #     },
//! #     verdict: Verdict::Pass,
//! #     findings: vec![],
//! #     summary: None,
//! # };
//! // Use default options
//! let md = render_markdown(&report);
//! let annotations = render_github_annotations(&report);
//!
//! // Or customize rendering
//! let options = RenderOptions {
//!     max_findings: 10,
//!     show_info: true,
//! };
//! let md = render_markdown_with_options(&report, &options);
//! let annotations = render_github_annotations_with_options(&report, &options);
//! ```

use builddiag_types::{Finding, Report, Severity, Verdict};

/// Options for controlling rendering behavior.
///
/// These options allow customization of how reports are rendered,
/// particularly for handling large reports with many findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Maximum number of findings to display.
    ///
    /// When the total number of findings exceeds this limit, a truncation
    /// note is added indicating how many findings were omitted.
    ///
    /// Default: 50
    pub max_findings: usize,

    /// Whether to include info-level findings in output.
    ///
    /// Info-level findings are typically informational and don't indicate
    /// problems. Setting this to `false` filters them out to reduce noise.
    ///
    /// Default: false
    pub show_info: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            max_findings: 50,
            show_info: false,
        }
    }
}

/// Renders a report as Markdown using default options.
///
/// This is a convenience wrapper around [`render_markdown_with_options`]
/// using [`RenderOptions::default()`].
///
/// # Output Format
///
/// The Markdown output includes:
/// - Summary header with verdict icon and counts
/// - Findings table sorted by severity (errors first)
/// - Location information (path:line) when available
/// - Check ID and finding code for each entry
/// - Truncation note when findings exceed budget
/// - Reproduce command for local verification
pub fn render_markdown(report: &Report) -> String {
    render_markdown_with_options(report, &RenderOptions::default())
}

/// Renders a report as Markdown with custom options.
///
/// See [`render_markdown`] for output format details.
///
/// # Arguments
///
/// * `report` - The report to render
/// * `options` - Rendering options controlling budget and filtering
///
/// # Example
///
/// ```
/// use builddiag_render::{render_markdown_with_options, RenderOptions};
/// # use builddiag_types::*;
/// # use chrono::Utc;
///
/// # let report = Report {
/// #     schema: Report::SCHEMA_V1.to_string(),
/// #     tool: ToolInfo { name: "builddiag".into(), version: "0.1.0".into() },
/// #     run: RunInfo {
/// #         started_at: Utc::now(),
/// #         ended_at: None,
/// #         duration_ms: 100,
/// #         host: HostInfo { os: "linux".into(), arch: "x86_64".into() },
/// #         git: None,
/// #     },
/// #     verdict: Verdict::Pass,
/// #     findings: vec![],
/// #     summary: None,
/// # };
/// let options = RenderOptions {
///     max_findings: 25,
///     show_info: true,
/// };
/// let md = render_markdown_with_options(&report, &options);
/// ```
pub fn render_markdown_with_options(report: &Report, options: &RenderOptions) -> String {
    let mut out = String::new();

    // Calculate counts from findings
    let (error_count, warn_count, _info_count) = count_by_severity(&report.findings);

    // Header with verdict icon and summary counts
    let icon = match report.verdict {
        Verdict::Pass => "✅",
        Verdict::Warn => "⚠️",
        Verdict::Fail => "❌",
        Verdict::Skip => "⏭️",
        Verdict::Error => "💥",
    };

    out.push_str(&format!(
        "## builddiag: {} {:?} ({} errors, {} warnings)\n\n",
        icon, report.verdict, error_count, warn_count
    ));

    // Filter and collect findings
    let mut findings: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| options.show_info || f.severity != Severity::Info)
        .collect();

    // Sort: severity desc (Error > Warn > Info), then check_id, then path, then line
    findings.sort_by(|a, b| {
        let sa = severity_rank(a.severity);
        let sb = severity_rank(b.severity);
        sb.cmp(&sa)
            .then_with(|| a.check_id.cmp(&b.check_id))
            .then_with(|| {
                let path_a = a.location.as_ref().map(|l| l.path.as_str()).unwrap_or("");
                let path_b = b.location.as_ref().map(|l| l.path.as_str()).unwrap_or("");
                path_a.cmp(path_b)
            })
            .then_with(|| {
                let line_a = a.location.as_ref().and_then(|l| l.line).unwrap_or(0);
                let line_b = b.location.as_ref().and_then(|l| l.line).unwrap_or(0);
                line_a.cmp(&line_b)
            })
    });

    if findings.is_empty() {
        out.push_str("No findings.\n");
        return out;
    }

    let total_count = findings.len();
    let truncated = total_count > options.max_findings;
    let display_count = total_count.min(options.max_findings);

    out.push_str("| severity | check | code | location | message |\n");
    out.push_str("|---|---|---|---|---|\n");

    for f in findings.into_iter().take(display_count) {
        let sev = match f.severity {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        };
        let loc = format_location(f);
        let msg = escape_md(&f.message);
        let code = escape_md(&f.code);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            sev, f.check_id, code, loc, msg
        ));
    }

    // Add truncation note if needed
    if truncated {
        let omitted = total_count - display_count;
        out.push_str(&format!(
            "\n*{} more finding{} truncated. See full report for details.*\n",
            omitted,
            if omitted == 1 { "" } else { "s" }
        ));
    }

    out.push_str("\nReproduce:\n");
    out.push_str("`builddiag check --root .`\n");

    out
}

/// Renders GitHub Actions annotations using default options.
///
/// This is a convenience wrapper around [`render_github_annotations_with_options`]
/// using [`RenderOptions::default()`].
///
/// # Output Format
///
/// Each annotation is formatted as:
/// ```text
/// ::error file={path},line={line}::[{check_id}:{code}] {message}
/// ::warning file={path},line={line}::[{check_id}:{code}] {message}
/// ::notice file={path},line={line}::[{check_id}:{code}] {message}
/// ```
///
/// Findings without a path or line are skipped as they cannot be displayed
/// as inline annotations in GitHub.
///
/// # Severity Mapping
///
/// - `Severity::Error` -> `::error`
/// - `Severity::Warn` -> `::warning`
/// - `Severity::Info` -> `::notice`
pub fn render_github_annotations(report: &Report) -> Vec<String> {
    render_github_annotations_with_options(report, &RenderOptions::default())
}

/// Renders GitHub Actions annotations with custom options.
///
/// See [`render_github_annotations`] for output format details.
///
/// # Arguments
///
/// * `report` - The report to render
/// * `options` - Rendering options controlling budget and filtering
///
/// # Budget Handling
///
/// GitHub has a limit on the number of annotations per run (typically 10 per step,
/// 50 per job). When findings exceed `max_findings`, only the first `max_findings`
/// annotations are returned (prioritized by severity).
pub fn render_github_annotations_with_options(
    report: &Report,
    options: &RenderOptions,
) -> Vec<String> {
    // Collect findings with location information
    let mut findings: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| {
            // Must have path and line for inline annotations
            f.location
                .as_ref()
                .map(|l| l.line.is_some())
                .unwrap_or(false)
        })
        .filter(|f| options.show_info || f.severity != Severity::Info)
        .collect();

    // Sort by severity (errors first), then check_id, path, line for deterministic output
    findings.sort_by(|a, b| {
        let sa = severity_rank(a.severity);
        let sb = severity_rank(b.severity);
        sb.cmp(&sa)
            .then_with(|| a.check_id.cmp(&b.check_id))
            .then_with(|| {
                let path_a = a.location.as_ref().map(|l| l.path.as_str()).unwrap_or("");
                let path_b = b.location.as_ref().map(|l| l.path.as_str()).unwrap_or("");
                path_a.cmp(path_b)
            })
            .then_with(|| {
                let line_a = a.location.as_ref().and_then(|l| l.line).unwrap_or(0);
                let line_b = b.location.as_ref().and_then(|l| l.line).unwrap_or(0);
                line_a.cmp(&line_b)
            })
    });

    // Apply budget limit
    let display_count = findings.len().min(options.max_findings);

    let mut lines = Vec::with_capacity(display_count);
    for f in findings.into_iter().take(display_count) {
        let kind = match f.severity {
            Severity::Error => "error",
            Severity::Warn => "warning",
            Severity::Info => "notice",
        };

        let loc = f.location.as_ref().unwrap(); // Safe: filtered above
        let path = &loc.path;
        let line = loc.line.unwrap_or(1);
        let message = escape_github_annotation(&f.message);
        let code = escape_github_annotation(&f.code);

        let annotation = if let Some(col) = loc.col {
            format!(
                "::{} file={},line={},col={}::[{}:{}] {}",
                kind, path, line, col, f.check_id, code, message
            )
        } else {
            format!(
                "::{} file={},line={}::[{}:{}] {}",
                kind, path, line, f.check_id, code, message
            )
        };
        lines.push(annotation);
    }
    lines
}

/// Counts findings by severity.
fn count_by_severity(findings: &[Finding]) -> (usize, usize, usize) {
    let mut error = 0;
    let mut warn = 0;
    let mut info = 0;
    for f in findings {
        match f.severity {
            Severity::Error => error += 1,
            Severity::Warn => warn += 1,
            Severity::Info => info += 1,
        }
    }
    (error, warn, info)
}

/// Returns a numeric rank for severity (higher = more severe).
fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Error => 3,
        Severity::Warn => 2,
        Severity::Info => 1,
    }
}

/// Formats the location for markdown display.
fn format_location(f: &Finding) -> String {
    match &f.location {
        Some(loc) => match loc.line {
            Some(l) => format!("{}:{}", escape_md(&loc.path), l),
            None => escape_md(&loc.path),
        },
        None => String::new(),
    }
}

/// Escapes special characters for Markdown table cells.
fn escape_md(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
        .replace('\r', "")
}

/// Escapes special characters for GitHub annotation messages.
///
/// GitHub annotations use `::` as delimiters, so we need to be careful
/// with special characters. Newlines are replaced with spaces.
fn escape_github_annotation(s: &str) -> String {
    s.replace('\n', " ").replace('\r', "").replace('%', "%25")
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::*;
    use chrono::Utc;

    fn create_test_report_empty() -> Report {
        Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "builddiag".into(),
                version: "0.1.0".into(),
            },
            run: RunInfo {
                started_at: Utc::now(),
                ended_at: None,
                duration_ms: 100,
                host: HostInfo {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                },
                git: None,
            },
            verdict: Verdict::Pass,
            findings: vec![],
            summary: None,
        }
    }

    fn create_test_report_with_findings() -> Report {
        let mut report = create_test_report_empty();
        report.findings = vec![
            Finding {
                check_id: "rust.msrv_defined".into(),
                code: "missing_msrv".into(),
                severity: Severity::Error,
                message: "Missing rust-version in Cargo.toml".into(),
                location: Some(Location {
                    path: "Cargo.toml".into(),
                    line: Some(1),
                    col: None,
                }),
                data: None,
            },
            Finding {
                check_id: "rust.toolchain_pinning".into(),
                code: "toolchain_not_pinned".into(),
                severity: Severity::Warn,
                message: "Toolchain is not pinned to a specific version".into(),
                location: Some(Location {
                    path: "rust-toolchain.toml".into(),
                    line: Some(2),
                    col: Some(5),
                }),
                data: None,
            },
            Finding {
                check_id: "rust.toolchain_pinning".into(),
                code: "toolchain_info".into(),
                severity: Severity::Info,
                message: "Detected toolchain channel: stable".into(),
                location: Some(Location {
                    path: "rust-toolchain.toml".into(),
                    line: Some(1),
                    col: None,
                }),
                data: None,
            },
        ];
        report.verdict = Verdict::Fail;
        report
    }

    fn create_test_report_many_findings(count: usize) -> Report {
        let mut report = create_test_report_empty();
        let mut findings = Vec::with_capacity(count);
        for i in 0..count {
            findings.push(Finding {
                check_id: "test.check".into(),
                code: format!("finding_{}", i),
                severity: if i % 3 == 0 {
                    Severity::Error
                } else if i % 3 == 1 {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                message: format!("Finding number {}", i),
                location: Some(Location {
                    path: format!("src/file_{}.rs", i),
                    line: Some((i + 1) as u32),
                    col: None,
                }),
                data: None,
            });
        }
        report.findings = findings;
        report.verdict = Verdict::Fail;
        report
    }

    #[test]
    fn test_render_options_default() {
        let options = RenderOptions::default();
        assert_eq!(options.max_findings, 50);
        assert!(!options.show_info);
    }

    #[test]
    fn test_markdown_empty_findings() {
        let report = create_test_report_empty();
        let md = render_markdown(&report);

        assert!(md.contains("## builddiag:"));
        assert!(md.contains("✅"));
        assert!(md.contains("Pass"));
        assert!(md.contains("0 errors"));
        assert!(md.contains("0 warnings"));
        assert!(md.contains("No findings."));
        assert!(!md.contains("Reproduce:"));
    }

    #[test]
    fn test_markdown_with_findings() {
        let report = create_test_report_with_findings();
        let md = render_markdown(&report);

        assert!(md.contains("## builddiag:"));
        assert!(md.contains("❌"));
        assert!(md.contains("Fail"));
        assert!(md.contains("1 errors"));
        assert!(md.contains("1 warnings"));

        // Check table structure
        assert!(md.contains("| severity | check | code | location | message |"));
        assert!(md.contains("|---|---|---|---|---|"));

        // Check that error finding is present
        assert!(md.contains("| error |"));
        assert!(md.contains("rust.msrv_defined"));
        assert!(md.contains("missing_msrv"));
        assert!(md.contains("Cargo.toml:1"));
        assert!(md.contains("Missing rust-version"));

        // Check that warn finding is present
        assert!(md.contains("| warn |"));
        assert!(md.contains("rust.toolchain_pinning"));
        assert!(md.contains("toolchain_not_pinned"));

        // Info findings should NOT be present by default
        assert!(!md.contains("| info |"));
        assert!(!md.contains("toolchain_info"));

        // Reproduce section
        assert!(md.contains("Reproduce:"));
        assert!(md.contains("`builddiag check --root .`"));
    }

    #[test]
    fn test_markdown_show_info() {
        let report = create_test_report_with_findings();
        let options = RenderOptions {
            max_findings: 50,
            show_info: true,
        };
        let md = render_markdown_with_options(&report, &options);

        // Info findings should now be present
        assert!(md.contains("| info |"));
        assert!(md.contains("toolchain_info"));
    }

    #[test]
    fn test_markdown_truncation() {
        let report = create_test_report_many_findings(100);
        let options = RenderOptions {
            max_findings: 10,
            show_info: true, // Include info to have more findings
        };
        let md = render_markdown_with_options(&report, &options);

        // Should have truncation note
        assert!(md.contains("more finding"));
        assert!(md.contains("truncated"));
        assert!(md.contains("See full report"));

        // Count the number of table rows (excluding header and separator)
        let table_rows = md
            .lines()
            .filter(|l| {
                l.starts_with("| error") || l.starts_with("| warn") || l.starts_with("| info")
            })
            .count();
        assert_eq!(table_rows, 10);
    }

    #[test]
    fn test_markdown_truncation_singular() {
        // Create exactly max_findings + 1 findings
        let report = create_test_report_many_findings(11);
        let options = RenderOptions {
            max_findings: 10,
            show_info: true,
        };
        let md = render_markdown_with_options(&report, &options);

        // Should have singular "finding" not "findings"
        assert!(md.contains("1 more finding truncated"));
    }

    #[test]
    fn test_markdown_escape_pipe() {
        let mut report = create_test_report_empty();
        report.findings = vec![Finding {
            check_id: "test.check".into(),
            code: "test|code".into(),
            severity: Severity::Error,
            message: "Message with | pipe character".into(),
            location: Some(Location {
                path: "path|with|pipes.rs".into(),
                line: Some(1),
                col: None,
            }),
            data: None,
        }];
        report.verdict = Verdict::Fail;

        let md = render_markdown(&report);

        // Pipes should be escaped
        assert!(md.contains("test\\|code"));
        assert!(md.contains("Message with \\| pipe"));
        assert!(md.contains("path\\|with\\|pipes.rs"));
    }

    #[test]
    fn test_github_annotations_empty() {
        let report = create_test_report_empty();
        let annotations = render_github_annotations(&report);

        assert!(annotations.is_empty());
    }

    #[test]
    fn test_github_annotations_with_findings() {
        let report = create_test_report_with_findings();
        let annotations = render_github_annotations(&report);

        // Should have 2 annotations (error and warn, not info by default)
        assert_eq!(annotations.len(), 2);

        // Error should come first (sorted by severity)
        assert!(annotations[0].starts_with("::error"));
        assert!(annotations[0].contains("file=Cargo.toml"));
        assert!(annotations[0].contains("line=1"));
        assert!(annotations[0].contains("[rust.msrv_defined:missing_msrv]"));
        assert!(annotations[0].contains("Missing rust-version"));

        // Warning should come second
        assert!(annotations[1].starts_with("::warning"));
        assert!(annotations[1].contains("file=rust-toolchain.toml"));
        assert!(annotations[1].contains("line=2"));
        assert!(annotations[1].contains("col=5"));
        assert!(annotations[1].contains("[rust.toolchain_pinning:toolchain_not_pinned]"));
    }

    #[test]
    fn test_github_annotations_show_info() {
        let report = create_test_report_with_findings();
        let options = RenderOptions {
            max_findings: 50,
            show_info: true,
        };
        let annotations = render_github_annotations_with_options(&report, &options);

        // Should now have 3 annotations including notice
        assert_eq!(annotations.len(), 3);

        // Check that notice is present
        let has_notice = annotations.iter().any(|a| a.starts_with("::notice"));
        assert!(has_notice);
    }

    #[test]
    fn test_github_annotations_budget() {
        let report = create_test_report_many_findings(100);
        let options = RenderOptions {
            max_findings: 10,
            show_info: true,
        };
        let annotations = render_github_annotations_with_options(&report, &options);

        // Should be limited to 10
        assert_eq!(annotations.len(), 10);

        // All should be errors (highest severity, sorted first)
        for ann in &annotations {
            assert!(ann.starts_with("::error"));
        }
    }

    #[test]
    fn test_github_annotations_skips_findings_without_location() {
        let mut report = create_test_report_empty();
        report.findings = vec![
            Finding {
                check_id: "test.check".into(),
                code: "no_location".into(),
                severity: Severity::Error,
                message: "Finding without location".into(),
                location: None,
                data: None,
            },
            Finding {
                check_id: "test.check".into(),
                code: "no_line".into(),
                severity: Severity::Error,
                message: "Finding without line".into(),
                location: Some(Location {
                    path: "file.rs".into(),
                    line: None,
                    col: None,
                }),
                data: None,
            },
            Finding {
                check_id: "test.check".into(),
                code: "with_location".into(),
                severity: Severity::Error,
                message: "Finding with location".into(),
                location: Some(Location {
                    path: "file.rs".into(),
                    line: Some(10),
                    col: None,
                }),
                data: None,
            },
        ];

        let annotations = render_github_annotations(&report);

        // Only one annotation (the one with both path and line)
        assert_eq!(annotations.len(), 1);
        assert!(annotations[0].contains("with_location"));
    }

    #[test]
    fn test_deterministic_output() {
        let report = create_test_report_with_findings();

        // Render multiple times
        let md1 = render_markdown(&report);
        let md2 = render_markdown(&report);
        let md3 = render_markdown(&report);

        assert_eq!(md1, md2);
        assert_eq!(md2, md3);

        let ann1 = render_github_annotations(&report);
        let ann2 = render_github_annotations(&report);

        assert_eq!(ann1, ann2);
    }

    #[test]
    fn test_verdict_icons() {
        let mut report = create_test_report_empty();

        report.verdict = Verdict::Pass;
        assert!(render_markdown(&report).contains("✅"));

        report.verdict = Verdict::Warn;
        assert!(render_markdown(&report).contains("⚠️"));

        report.verdict = Verdict::Fail;
        assert!(render_markdown(&report).contains("❌"));

        report.verdict = Verdict::Skip;
        assert!(render_markdown(&report).contains("⏭️"));

        report.verdict = Verdict::Error;
        assert!(render_markdown(&report).contains("💥"));
    }

    #[test]
    fn test_format_location() {
        let f1 = Finding {
            check_id: "test".into(),
            code: "test".into(),
            severity: Severity::Error,
            message: "test".into(),
            location: Some(Location {
                path: "file.rs".into(),
                line: Some(10),
                col: None,
            }),
            data: None,
        };
        assert_eq!(format_location(&f1), "file.rs:10");

        let f2 = Finding {
            check_id: "test".into(),
            code: "test".into(),
            severity: Severity::Error,
            message: "test".into(),
            location: Some(Location {
                path: "file.rs".into(),
                line: None,
                col: None,
            }),
            data: None,
        };
        assert_eq!(format_location(&f2), "file.rs");

        let f3 = Finding {
            check_id: "test".into(),
            code: "test".into(),
            severity: Severity::Error,
            message: "test".into(),
            location: None,
            data: None,
        };
        assert_eq!(format_location(&f3), "");
    }

    #[test]
    fn test_escape_md() {
        assert_eq!(escape_md("hello | world"), "hello \\| world");
        assert_eq!(escape_md("back\\slash"), "back\\\\slash");
        assert_eq!(escape_md("line\nbreak"), "line break");
        assert_eq!(escape_md("carriage\rreturn"), "carriagereturn");
    }

    #[test]
    fn test_escape_github_annotation() {
        assert_eq!(escape_github_annotation("hello\nworld"), "hello world");
        assert_eq!(escape_github_annotation("50%"), "50%25");
    }

    #[test]
    fn markdown_smoke() {
        let report = Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "builddiag".into(),
                version: "0.1.0".into(),
            },
            run: RunInfo {
                started_at: Utc::now(),
                ended_at: None,
                duration_ms: 100,
                host: HostInfo {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                },
                git: None,
            },
            verdict: Verdict::Fail,
            findings: vec![Finding {
                check_id: "rust.msrv_defined".into(),
                code: "missing".into(),
                severity: Severity::Error,
                message: "Missing MSRV".into(),
                location: Some(Location {
                    path: "Cargo.toml".into(),
                    line: Some(1),
                    col: None,
                }),
                data: None,
            }],
            summary: None,
        };
        let md = render_markdown(&report);
        assert!(md.contains("Missing MSRV"));
        let ann = render_github_annotations(&report);
        assert!(!ann.is_empty());
    }
}
