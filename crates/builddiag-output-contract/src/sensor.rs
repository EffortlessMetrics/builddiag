use anyhow::{Context, Result, ensure};
use builddiag_types::{SensorFinding, SensorReport, Severity};
use std::path::Path;

/// Parse a sensor report from a path and return strongly typed output.
pub fn load_sensor_report(path: impl AsRef<Path>) -> Result<SensorReport> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read sensor report at {:?}", path))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse sensor report JSON at {:?}", path))
}

/// Validate a sensor report against the runtime contract.
pub fn validate_sensor_report(report: &SensorReport) -> Result<()> {
    ensure!(
        report.schema == SensorReport::SCHEMA_V1,
        "sensor schema must be {} (got {})",
        SensorReport::SCHEMA_V1,
        report.schema
    );
    ensure!(
        report.tool.is_some(),
        "sensor report.tool should be present"
    );
    ensure!(report.run.is_some(), "sensor report.run should be present");

    if let Some(run) = &report.run
        && let Some(ended_at) = run.ended_at
    {
        ensure!(
            ended_at >= run.started_at,
            "sensor.run.ended_at should be >= sensor.run.started_at"
        );
    }

    let mut info_count = 0usize;
    let mut warn_count = 0usize;
    let mut error_count = 0usize;
    for (index, finding) in report.findings.iter().enumerate() {
        validate_sensor_finding(finding, index)?;
        match finding.severity {
            Severity::Info => info_count += 1,
            Severity::Warn => warn_count += 1,
            Severity::Error => error_count += 1,
        }
    }

    ensure!(
        report.verdict.counts.info == info_count,
        "sensor verdict count info should match findings ({} != {})",
        report.verdict.counts.info,
        info_count
    );
    ensure!(
        report.verdict.counts.warn == warn_count,
        "sensor verdict count warn should match findings ({} != {})",
        report.verdict.counts.warn,
        warn_count
    );
    ensure!(
        report.verdict.counts.error == error_count,
        "sensor verdict count error should match findings ({} != {})",
        report.verdict.counts.error,
        error_count
    );

    Ok(())
}

/// Parse and validate in one call.
pub fn load_and_validate_sensor_report(path: impl AsRef<Path>) -> Result<SensorReport> {
    let report = load_sensor_report(path)?;
    validate_sensor_report(&report)?;
    Ok(report)
}

fn validate_sensor_finding(finding: &SensorFinding, index: usize) -> Result<()> {
    ensure!(
        !finding.check_id.trim().is_empty(),
        "sensor finding[{}].check_id should be non-empty",
        index
    );
    ensure!(
        !finding.code.trim().is_empty(),
        "sensor finding[{}].code should be non-empty",
        index
    );
    ensure!(
        !finding.message.trim().is_empty(),
        "sensor finding[{}].message should be non-empty",
        index
    );
    ensure!(
        finding.fingerprint.len() == 64
            && finding.fingerprint.chars().all(|c| c.is_ascii_hexdigit()),
        "sensor finding fingerprint should be 64-char hex: {}",
        finding.fingerprint
    );
    if let Some(location) = &finding.location {
        ensure!(
            !location.path.contains('\\'),
            "sensor finding path should use forward slashes: {}",
            location.path
        );
    }
    Ok(())
}
