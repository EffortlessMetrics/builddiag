use anyhow::{Context, Result, ensure};
use builddiag_types::Report;
use std::path::Path;

/// Parse a builddiag report from a path and return strongly typed output.
pub fn load_builddiag_report(path: impl AsRef<Path>) -> Result<Report> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read report at {:?}", path))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse builddiag report JSON at {:?}", path))
}

/// Validate a builddiag report against the runtime contract.
pub fn validate_builddiag_report(report: &Report) -> Result<()> {
    ensure!(
        report.schema == Report::SCHEMA_V1,
        "report.schema must be {} (got {})",
        Report::SCHEMA_V1,
        report.schema
    );
    ensure!(report.tool.is_some(), "report.tool should be present");
    ensure!(report.run.is_some(), "report.run should be present");

    if let Some(run) = &report.run
        && let Some(ended_at) = run.ended_at
    {
        ensure!(
            ended_at >= run.started_at,
            "run.ended_at should be >= run.started_at"
        );
    }

    for (index, finding) in report.findings.iter().enumerate() {
        ensure!(
            !finding.check_id.trim().is_empty(),
            "finding[{}].check_id should be non-empty",
            index
        );
        ensure!(
            !finding.code.trim().is_empty(),
            "finding[{}].code should be non-empty",
            index
        );
        ensure!(
            !finding.message.trim().is_empty(),
            "finding[{}].message should be non-empty",
            index
        );
        if let Some(location) = &finding.location {
            ensure!(
                !location.path.contains('\\'),
                "finding.location.path should use forward slashes: {}",
                location.path
            );
        }
    }

    if let Some(summary) = &report.summary {
        ensure!(
            summary.total_findings == report.findings.len(),
            "summary.total_findings should match findings length"
        );
        ensure!(
            summary.by_severity.values().sum::<usize>() == report.findings.len(),
            "summary.by_severity total should match findings length"
        );
    }

    Ok(())
}

/// Parse and validate in one call.
pub fn load_and_validate_builddiag_report(path: impl AsRef<Path>) -> Result<Report> {
    let report = load_builddiag_report(path.as_ref())?;
    validate_builddiag_report(&report)?;
    Ok(report)
}
