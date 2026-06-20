//! Receipt and sensor-contract primitives for builddiag.
//!
//! This crate packages the transformation from builddiag-native reports to
//! Cockpit-compatible `sensor.report.v1` artifacts.

use anyhow::Error;
use builddiag_domain::{build_sensor_verdict, finding_to_sensor};
use builddiag_types::{
    Artifact, Capability, CheckReport, GitInfo, Report, RunInfo, SENSOR_REPORT_SCHEMA_V1,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Build capability state for the current run.
pub fn build_capabilities(
    config: &builddiag_types::Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
) -> BTreeMap<String, Capability> {
    build_capabilities_inner(
        config,
        git_info,
        has_toolchain,
        has_checksums,
        diff_aware_used,
        false,
    )
}

/// Build capability state and include explicit `substrate` marking when substrate mode was used.
#[cfg(feature = "with-substrate")]
pub fn build_capabilities_with_substrate(
    config: &builddiag_types::Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
    substrate_used: bool,
) -> BTreeMap<String, Capability> {
    build_capabilities_inner(
        config,
        git_info,
        has_toolchain,
        has_checksums,
        diff_aware_used,
        substrate_used,
    )
}

#[cfg(not(feature = "with-substrate"))]
pub fn build_capabilities_with_substrate(
    config: &builddiag_types::Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
    substrate_used: bool,
) -> BTreeMap<String, Capability> {
    let mut caps = build_capabilities_inner(
        config,
        git_info,
        has_toolchain,
        has_checksums,
        diff_aware_used,
        false,
    );
    let _ = substrate_used;
    caps
}

fn build_capabilities_inner(
    config: &builddiag_types::Config,
    git_info: Option<&GitInfo>,
    has_toolchain: bool,
    has_checksums: bool,
    diff_aware_used: bool,
    substrate_used: bool,
) -> BTreeMap<String, Capability> {
    let mut caps = BTreeMap::new();

    if git_info.is_some() {
        caps.insert("git".to_string(), Capability::available());
    } else {
        caps.insert(
            "git".to_string(),
            Capability::unavailable("git repository not detected"),
        );
    }

    caps.insert("config".to_string(), Capability::available());

    if has_toolchain {
        caps.insert("toolchain".to_string(), Capability::available());
    } else {
        caps.insert(
            "toolchain".to_string(),
            Capability::unavailable("rust-toolchain.toml not found"),
        );
    }

    if has_checksums {
        caps.insert("checksums".to_string(), Capability::available());
    } else if !config.policy.checksums.require_file {
        caps.insert(
            "checksums".to_string(),
            Capability::skipped("checksums not required by config"),
        );
    } else {
        caps.insert(
            "checksums".to_string(),
            Capability::unavailable("checksums file not found"),
        );
    }

    if diff_aware_used {
        caps.insert("diff_aware".to_string(), Capability::available());
    } else if config.defaults.diff_aware {
        caps.insert(
            "diff_aware".to_string(),
            Capability::unavailable("could not compute git diff"),
        );
    } else {
        caps.insert(
            "diff_aware".to_string(),
            Capability::skipped("diff-aware mode not enabled"),
        );
    }

    if substrate_used {
        caps.insert("substrate".to_string(), Capability::available());
    }

    caps
}

/// Convert a builddiag report into `sensor.report.v1`.
pub fn report_to_sensor(
    report: &Report,
    checks: &[CheckReport],
    capabilities: BTreeMap<String, Capability>,
    artifacts: Vec<Artifact>,
) -> builddiag_types::SensorReport {
    let sensor_findings = report
        .findings
        .iter()
        .map(|f| finding_to_sensor(f, None, None))
        .collect();

    let sensor_run = report
        .run
        .as_ref()
        .map(|run| builddiag_types::SensorRunInfo {
            started_at: run.started_at,
            ended_at: run.ended_at,
            duration_ms: run.duration_ms,
            host: run.host.clone(),
            git: run.git.clone(),
            capabilities,
        });

    builddiag_types::SensorReport {
        schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
        tool: report.tool.clone(),
        run: sensor_run,
        verdict: build_sensor_verdict(report.verdict, checks),
        findings: sensor_findings,
        artifacts,
        data: report.data.clone(),
    }
}

/// Build a stable runtime error receipt.
pub fn create_error_receipt(started_at: DateTime<Utc>, error: &Error) -> Report {
    let end = Utc::now();

    Report {
        schema: Report::SCHEMA_V1.to_string(),
        tool: Some(builddiag_types::ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        run: Some(RunInfo {
            started_at,
            ended_at: Some(end),
            duration_ms: (end - started_at).num_milliseconds().max(0) as u64,
            host: builddiag_types::HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            git: None,
        }),
        verdict: builddiag_types::Verdict::Error,
        findings: vec![builddiag_types::Finding {
            check_id: "tool.runtime".to_string(),
            code: "runtime_error".to_string(),
            severity: builddiag_types::Severity::Error,
            message: format!("Internal error: {error:#}"),
            location: None,
        }],
        summary: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{CapabilityStatus, CheckStatus, Location, Summary, VerdictStatus};
    use std::collections::BTreeMap;

    #[test]
    fn build_capabilities_with_all_available() {
        let config = builddiag_types::Config::default();
        let git = GitInfo {
            commit: "abc123".to_string(),
            branch: Some("main".to_string()),
            dirty: false,
        };

        let caps = build_capabilities(&config, Some(&git), true, true, true);

        assert_eq!(caps.get("git").unwrap().status, CapabilityStatus::Available);
        assert_eq!(
            caps.get("toolchain").unwrap().status,
            CapabilityStatus::Available
        );
        assert_eq!(
            caps.get("checksums").unwrap().status,
            CapabilityStatus::Available
        );
        assert_eq!(
            caps.get("diff_aware").unwrap().status,
            CapabilityStatus::Available
        );
    }

    #[test]
    fn build_capabilities_with_none_available() {
        let mut config = builddiag_types::Config::default();
        config.policy.checksums.require_file = true;
        config.defaults.diff_aware = true;

        let caps = build_capabilities(&config, None, false, false, false);

        assert_eq!(
            caps.get("git").unwrap().status,
            CapabilityStatus::Unavailable
        );
        assert_eq!(
            caps.get("toolchain").unwrap().status,
            CapabilityStatus::Unavailable
        );
        assert_eq!(
            caps.get("checksums").unwrap().status,
            CapabilityStatus::Unavailable
        );
        assert_eq!(
            caps.get("diff_aware").unwrap().status,
            CapabilityStatus::Unavailable
        );
    }

    #[test]
    fn build_capabilities_with_skipped() {
        let mut config = builddiag_types::Config::default();
        config.policy.checksums.require_file = false;
        config.defaults.diff_aware = false;

        let caps = build_capabilities(&config, None, false, false, false);

        assert_eq!(
            caps.get("checksums").unwrap().status,
            CapabilityStatus::Skipped
        );
        assert_eq!(
            caps.get("diff_aware").unwrap().status,
            CapabilityStatus::Skipped
        );
    }

    #[test]
    fn create_error_receipt_creates_valid_report() {
        let start = Utc::now();
        let error: Error = anyhow::anyhow!("Test error message");

        let receipt = create_error_receipt(start, &error);

        assert_eq!(receipt.schema, Report::SCHEMA_V1);
        assert_eq!(receipt.verdict, builddiag_types::Verdict::Error);
        assert_eq!(receipt.findings.len(), 1);
        assert_eq!(receipt.findings[0].check_id, "tool.runtime");
        assert_eq!(receipt.findings[0].code, "runtime_error");
        assert_eq!(
            receipt.findings[0].severity,
            builddiag_types::Severity::Error
        );
        assert!(receipt.findings[0].message.contains("Test error message"));
        assert!(receipt.tool.is_some());
        assert!(receipt.run.is_some());
    }

    #[test]
    fn report_to_sensor_converts_correctly() {
        let report = Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool: Some(builddiag_types::ToolInfo {
                name: "builddiag".to_string(),
                version: "0.1.0".to_string(),
            }),
            run: Some(RunInfo {
                started_at: Utc::now(),
                ended_at: Some(Utc::now()),
                duration_ms: 100,
                host: builddiag_types::HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                },
                git: Some(GitInfo {
                    commit: "abc123".to_string(),
                    branch: Some("main".to_string()),
                    dirty: false,
                }),
            }),
            verdict: builddiag_types::Verdict::Warn,
            findings: vec![builddiag_types::Finding {
                check_id: "test.check".to_string(),
                code: "test_code".to_string(),
                severity: builddiag_types::Severity::Warn,
                message: "Test warning".to_string(),
                location: Some(Location {
                    path: "file.rs".to_string(),
                    line: Some(10),
                    col: None,
                }),
            }],
            summary: Some(Summary {
                total_findings: 1,
                by_severity: BTreeMap::new(),
                by_check: BTreeMap::new(),
            }),
            data: None,
        };

        let checks = vec![CheckReport {
            id: "test.check".to_string(),
            status: CheckStatus::Warn,
            findings: report.findings.clone(),
            skipped_reason: None,
            skipped_detail: None,
        }];

        let mut caps = BTreeMap::new();
        caps.insert("git".to_string(), Capability::available());

        let sensor = report_to_sensor(&report, &checks, caps, vec![]);

        assert_eq!(sensor.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor.verdict.status, VerdictStatus::Warn);
        assert_eq!(sensor.findings.len(), 1);
        assert!(!sensor.findings[0].fingerprint.is_empty());
        assert!(sensor.run.is_some());
        assert!(!sensor.run.as_ref().unwrap().capabilities.is_empty());
    }

    #[cfg(feature = "with-substrate")]
    #[test]
    fn build_capabilities_with_substrate_marks_capability() {
        let config = builddiag_types::Config::default();
        let caps = build_capabilities_with_substrate(&config, None, false, false, false, true);
        assert!(caps.contains_key("substrate"));
    }

    #[cfg(not(feature = "with-substrate"))]
    #[test]
    fn build_capabilities_with_substrate_is_noop_without_feature() {
        let config = builddiag_types::Config::default();
        let caps = build_capabilities_with_substrate(&config, None, false, false, false, true);
        assert!(!caps.contains_key("substrate"));
    }
}
