//! Fuzz target for Markdown and GitHub annotation rendering.
//!
//! This fuzz target exercises the rendering functions with arbitrary Report
//! structures to discover crashes, panics, or unexpected behavior during output
//! generation.
//!
//! **Validates: Requirements 5.6 (Rendering resilience)**

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use builddiag_render::{render_github_annotations_with_options, render_markdown_with_options, RenderOptions};
use builddiag_types::{Finding, HostInfo, Location, Report, RunInfo, Severity, Summary, ToolInfo, Verdict};
use chrono::{TimeZone, Utc};
use std::collections::BTreeMap;

/// Arbitrary-derivable wrapper for Severity
#[derive(Debug, Clone, Copy, Arbitrary)]
enum FuzzSeverity {
    Info,
    Warn,
    Error,
}

impl From<FuzzSeverity> for Severity {
    fn from(s: FuzzSeverity) -> Self {
        match s {
            FuzzSeverity::Info => Severity::Info,
            FuzzSeverity::Warn => Severity::Warn,
            FuzzSeverity::Error => Severity::Error,
        }
    }
}

/// Arbitrary-derivable wrapper for Verdict
#[derive(Debug, Clone, Copy, Arbitrary)]
enum FuzzVerdict {
    Pass,
    Warn,
    Fail,
    Skip,
    Error,
}

impl From<FuzzVerdict> for Verdict {
    fn from(v: FuzzVerdict) -> Self {
        match v {
            FuzzVerdict::Pass => Verdict::Pass,
            FuzzVerdict::Warn => Verdict::Warn,
            FuzzVerdict::Fail => Verdict::Fail,
            FuzzVerdict::Skip => Verdict::Skip,
            FuzzVerdict::Error => Verdict::Error,
        }
    }
}

/// Fuzzable location structure
#[derive(Debug, Clone, Arbitrary)]
struct FuzzLocation {
    path: String,
    line: Option<u32>,
    col: Option<u32>,
}

impl From<FuzzLocation> for Location {
    fn from(l: FuzzLocation) -> Self {
        Location {
            path: l.path,
            line: l.line,
            col: l.col,
        }
    }
}

/// Fuzzable finding structure
#[derive(Debug, Clone, Arbitrary)]
struct FuzzFinding {
    check_id: String,
    code: String,
    severity: FuzzSeverity,
    message: String,
    location: Option<FuzzLocation>,
}

impl From<FuzzFinding> for Finding {
    fn from(f: FuzzFinding) -> Self {
        Finding {
            check_id: f.check_id,
            code: f.code,
            severity: f.severity.into(),
            message: f.message,
            location: f.location.map(|l| l.into()),
        }
    }
}

/// Fuzzable render options
#[derive(Debug, Clone, Arbitrary)]
struct FuzzRenderOptions {
    max_findings: u8, // Use u8 to keep values reasonable
    show_info: bool,
}

impl From<FuzzRenderOptions> for RenderOptions {
    fn from(o: FuzzRenderOptions) -> Self {
        RenderOptions {
            max_findings: o.max_findings.max(1) as usize, // At least 1
            show_info: o.show_info,
        }
    }
}

/// Main fuzz input structure
#[derive(Debug, Clone, Arbitrary)]
struct FuzzInput {
    verdict: FuzzVerdict,
    findings: Vec<FuzzFinding>,
    options: FuzzRenderOptions,
}

fuzz_target!(|input: FuzzInput| {
    // Build the Report structure
    let findings: Vec<Finding> = input.findings.into_iter().map(|f| f.into()).collect();

    // Create summary counts
    let mut by_severity = BTreeMap::new();
    let mut by_check = BTreeMap::new();
    for f in &findings {
        let sev_key = match f.severity {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        };
        *by_severity.entry(sev_key.to_string()).or_insert(0) += 1;
        *by_check.entry(f.check_id.clone()).or_insert(0) += 1;
    }

    let report = Report {
        schema: Report::SCHEMA_V1.to_string(),
        tool: Some(ToolInfo {
            name: "builddiag".to_string(),
            version: "0.1.0".to_string(),
        }),
        run: Some(RunInfo {
            started_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            duration_ms: 100,
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            git: None,
        }),
        verdict: input.verdict.into(),
        findings,
        summary: Some(Summary {
            total_findings: by_severity.values().sum(),
            by_severity,
            by_check,
        }),
        data: None,
    };

    let options: RenderOptions = input.options.into();

    // Test markdown rendering - should never panic
    let _md = render_markdown_with_options(&report, &options);

    // Test GitHub annotations - should never panic
    let _annotations = render_github_annotations_with_options(&report, &options);
});
