//! Baseline operations for builddiag findings.
//!
//! This crate provides deterministic baseline snapshot and filtering helpers used by
//! the CLI to support regression-only reporting workflows.

use anyhow::{Context, Result, anyhow};
use builddiag_domain::{check_status_from_findings, compute_fingerprint};
use builddiag_types::{CheckReport, CheckStatus, Finding, Report, Summary, Verdict};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Schema id for baseline files.
pub const BASELINE_SCHEMA_V1: &str = "builddiag.baseline.v1";

/// A single baseline entry keyed by finding fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Stable finding fingerprint.
    pub fingerprint: String,
    /// Check id that emitted the finding.
    pub check_id: String,
    /// Finding code.
    pub code: String,
    /// Repo-relative path, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 1-based line, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// On-disk baseline file format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// Schema id for this file.
    pub schema: String,
    /// Fingerprinted entries currently acknowledged by the repository.
    #[serde(default)]
    pub entries: Vec<BaselineEntry>,
}

impl Default for Baseline {
    fn default() -> Self {
        Self {
            schema: BASELINE_SCHEMA_V1.to_string(),
            entries: Vec::new(),
        }
    }
}

/// Result of filtering a report against a baseline.
#[derive(Debug, Clone)]
pub struct FilterResult {
    /// Filtered report containing only new findings.
    pub report: Report,
    /// Number of findings suppressed by baseline.
    pub suppressed: usize,
    /// Number of findings remaining after suppression.
    pub new_findings: usize,
}

/// Load a baseline file from disk.
pub fn read(path: &Utf8Path) -> Result<Baseline> {
    let raw = fs::read_to_string(path).with_context(|| format!("read baseline {path}"))?;
    let mut baseline: Baseline =
        serde_json::from_str(&raw).with_context(|| format!("parse baseline {path}"))?;
    normalize(&mut baseline)?;
    Ok(baseline)
}

/// Load a baseline file, returning an empty baseline when the file does not exist.
pub fn read_or_default(path: &Utf8Path) -> Result<Baseline> {
    match read(path) {
        Ok(b) => Ok(b),
        Err(err) if is_not_found(&err) => Ok(Baseline::default()),
        Err(err) => Err(err),
    }
}

/// Persist a baseline file atomically.
pub fn write(path: &Utf8Path, baseline: &Baseline) -> Result<()> {
    let mut normalized = baseline.clone();
    normalize(&mut normalized)?;

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("no parent directory for baseline path {path}"))?;
    fs::create_dir_all(parent).with_context(|| format!("create {parent}"))?;

    let bytes =
        serde_json::to_vec_pretty(&normalized).with_context(|| format!("serialize {path}"))?;
    let tmp = parent.join(format!(".{}.tmp", path.file_name().unwrap_or("baseline")));
    fs::write(&tmp, bytes).with_context(|| format!("write {tmp}"))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {tmp} -> {path}"))?;
    Ok(())
}

/// Build a new baseline from the findings in a report.
pub fn from_report(report: &Report) -> Baseline {
    let mut entries = BTreeMap::<String, BaselineEntry>::new();

    for finding in &report.findings {
        let fingerprint = compute_fingerprint(finding);
        entries
            .entry(fingerprint.clone())
            .or_insert_with(|| BaselineEntry {
                fingerprint,
                check_id: finding.check_id.clone(),
                code: finding.code.clone(),
                path: finding.location.as_ref().map(|loc| loc.path.clone()),
                line: finding.location.as_ref().and_then(|loc| loc.line),
            });
    }

    Baseline {
        schema: BASELINE_SCHEMA_V1.to_string(),
        entries: entries.into_values().collect(),
    }
}

/// Merge report findings into an existing baseline.
///
/// Returns the number of newly-added baseline entries.
pub fn merge_report(baseline: &mut Baseline, report: &Report) -> Result<usize> {
    normalize(baseline)?;
    let before = baseline.entries.len();

    let mut known: BTreeSet<String> = baseline
        .entries
        .iter()
        .map(|entry| entry.fingerprint.clone())
        .collect();

    for finding in &report.findings {
        let fingerprint = compute_fingerprint(finding);
        if known.insert(fingerprint.clone()) {
            baseline.entries.push(BaselineEntry {
                fingerprint,
                check_id: finding.check_id.clone(),
                code: finding.code.clone(),
                path: finding.location.as_ref().map(|loc| loc.path.clone()),
                line: finding.location.as_ref().and_then(|loc| loc.line),
            });
        }
    }

    normalize(baseline)?;
    Ok(baseline.entries.len().saturating_sub(before))
}

/// Filter a report so it only contains findings not present in the baseline.
pub fn filter_report(report: &Report, baseline: &Baseline) -> Result<FilterResult> {
    if report.verdict == Verdict::Error {
        return Ok(FilterResult {
            report: report.clone(),
            suppressed: 0,
            new_findings: report.findings.len(),
        });
    }

    let mut normalized = baseline.clone();
    normalize(&mut normalized)?;
    let known: BTreeSet<&str> = normalized
        .entries
        .iter()
        .map(|entry| entry.fingerprint.as_str())
        .collect();

    let mut kept = Vec::with_capacity(report.findings.len());
    let mut suppressed = 0usize;

    for finding in &report.findings {
        let fingerprint = compute_fingerprint(finding);
        if known.contains(fingerprint.as_str()) {
            suppressed += 1;
        } else {
            kept.push(finding.clone());
        }
    }

    let verdict = verdict_from_findings(&kept);
    let summary = summary_from_findings(&kept);
    let mut filtered = report.clone();
    filtered.findings = kept;
    filtered.verdict = verdict;
    filtered.summary = Some(summary);

    Ok(FilterResult {
        new_findings: filtered.findings.len(),
        report: filtered,
        suppressed,
    })
}

/// Rebuild check reports from a flat findings list for sensor verdict generation.
pub fn checks_from_findings(findings: &[Finding]) -> Vec<CheckReport> {
    let mut grouped = BTreeMap::<String, Vec<Finding>>::new();
    for finding in findings {
        grouped
            .entry(finding.check_id.clone())
            .or_default()
            .push(finding.clone());
    }

    grouped
        .into_iter()
        .map(|(id, findings)| CheckReport {
            status: status_from_findings(&findings),
            id,
            findings,
            skipped_reason: None,
            skipped_detail: None,
        })
        .collect()
}

fn status_from_findings(findings: &[Finding]) -> CheckStatus {
    check_status_from_findings(findings)
}

/// Compute verdict from a findings slice.
pub fn verdict_from_findings(findings: &[Finding]) -> Verdict {
    if findings
        .iter()
        .any(|finding| matches!(finding.severity, builddiag_types::Severity::Error))
    {
        Verdict::Fail
    } else if findings
        .iter()
        .any(|finding| matches!(finding.severity, builddiag_types::Severity::Warn))
    {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}

/// Build report summary from a flat findings slice.
pub fn summary_from_findings(findings: &[Finding]) -> Summary {
    let mut by_severity: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_check: BTreeMap<String, usize> = BTreeMap::new();

    for finding in findings {
        let sev_key = match finding.severity {
            builddiag_types::Severity::Info => "info",
            builddiag_types::Severity::Warn => "warn",
            builddiag_types::Severity::Error => "error",
        };
        *by_severity.entry(sev_key.to_string()).or_insert(0) += 1;
        *by_check.entry(finding.check_id.clone()).or_insert(0) += 1;
    }

    Summary {
        total_findings: findings.len(),
        by_severity,
        by_check,
    }
}

fn normalize(baseline: &mut Baseline) -> Result<()> {
    if baseline.schema != BASELINE_SCHEMA_V1 {
        return Err(anyhow!(
            "invalid baseline schema '{}', expected '{}'",
            baseline.schema,
            BASELINE_SCHEMA_V1
        ));
    }

    baseline
        .entries
        .sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
    baseline
        .entries
        .dedup_by(|a, b| a.fingerprint == b.fingerprint);
    Ok(())
}

fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{Location, Severity};
    use tempfile::TempDir;

    fn sample_report() -> Report {
        Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool: None,
            run: None,
            verdict: Verdict::Fail,
            findings: vec![
                Finding {
                    check_id: "rust.msrv_defined".to_string(),
                    code: "missing_msrv".to_string(),
                    severity: Severity::Error,
                    message: "missing".to_string(),
                    location: Some(Location {
                        path: "Cargo.toml".to_string(),
                        line: Some(1),
                        col: None,
                    }),
                },
                Finding {
                    check_id: "workspace.member_ordering".to_string(),
                    code: "members_not_sorted".to_string(),
                    severity: Severity::Warn,
                    message: "warn".to_string(),
                    location: Some(Location {
                        path: "Cargo.toml".to_string(),
                        line: Some(2),
                        col: None,
                    }),
                },
            ],
            summary: None,
            data: None,
        }
    }

    #[test]
    fn round_trip_read_write_baseline() {
        let temp = TempDir::new().unwrap();
        let path = Utf8Path::from_path(temp.path())
            .unwrap()
            .join(".builddiag-baseline.json");
        let baseline = from_report(&sample_report());
        write(&path, &baseline).unwrap();
        let loaded = read(&path).unwrap();
        assert_eq!(loaded.schema, BASELINE_SCHEMA_V1);
        assert_eq!(loaded.entries.len(), 2);
    }

    #[test]
    fn filter_report_suppresses_known_findings() {
        let report = sample_report();
        let baseline = from_report(&report);
        let filtered = filter_report(&report, &baseline).unwrap();
        assert_eq!(filtered.suppressed, 2);
        assert_eq!(filtered.new_findings, 0);
        assert_eq!(filtered.report.verdict, Verdict::Pass);
        assert!(filtered.report.findings.is_empty());
    }

    #[test]
    fn merge_report_adds_only_new_entries() {
        let report = sample_report();
        let mut baseline = Baseline::default();
        let added = merge_report(&mut baseline, &report).unwrap();
        assert_eq!(added, 2);
        let second = merge_report(&mut baseline, &report).unwrap();
        assert_eq!(second, 0);
        assert_eq!(baseline.entries.len(), 2);
    }
}
