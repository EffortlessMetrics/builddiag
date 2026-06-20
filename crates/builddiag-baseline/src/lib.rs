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

/// Result of filtering a report using inline suppression comments in Cargo.toml.
#[derive(Debug, Clone)]
pub struct InlineSuppressionResult {
    /// Filtered report with matching findings removed.
    pub report: Report,
    /// Number of findings suppressed by inline comments.
    pub suppressed: usize,
    /// Number of findings remaining after suppression.
    pub remaining_findings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SuppressionSelector {
    Any,
    Check(String),
    Code(String),
    CheckAndCode { check_id: String, code: String },
}

#[derive(Debug, Clone, Default)]
struct FileSuppressions {
    file_scoped: Vec<SuppressionSelector>,
    line_scoped: BTreeMap<u32, Vec<SuppressionSelector>>,
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

/// Filter a report using inline `builddiag:ignore` comments in Cargo.toml files.
///
/// Supported selector forms:
/// - `# builddiag:ignore` (suppress all findings for the file or line)
/// - `# builddiag:ignore missing_msrv` (suppress by finding code)
/// - `# builddiag:ignore rust.msrv_defined` (suppress by check id)
/// - `# builddiag:ignore rust.msrv_defined:missing_msrv` (suppress by check+code)
///
/// A directive on a comment-only line is file-scoped. A directive on a line that
/// also contains TOML content is line-scoped.
pub fn filter_report_inline_suppressions(
    root: &Utf8Path,
    report: &Report,
) -> Result<InlineSuppressionResult> {
    if report.verdict == Verdict::Error {
        return Ok(InlineSuppressionResult {
            report: report.clone(),
            suppressed: 0,
            remaining_findings: report.findings.len(),
        });
    }

    let suppressions = collect_inline_suppressions(root, report)?;
    if suppressions.is_empty() {
        return Ok(InlineSuppressionResult {
            report: report.clone(),
            suppressed: 0,
            remaining_findings: report.findings.len(),
        });
    }

    let mut kept = Vec::with_capacity(report.findings.len());
    let mut suppressed = 0usize;

    for finding in &report.findings {
        if finding_matches_suppression(finding, &suppressions) {
            suppressed += 1;
        } else {
            kept.push(finding.clone());
        }
    }

    let mut filtered = report.clone();
    filtered.findings = kept;
    filtered.verdict = verdict_from_findings(&filtered.findings);
    filtered.summary = Some(summary_from_findings(&filtered.findings));

    Ok(InlineSuppressionResult {
        remaining_findings: filtered.findings.len(),
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

fn collect_inline_suppressions(
    root: &Utf8Path,
    report: &Report,
) -> Result<BTreeMap<String, FileSuppressions>> {
    let cargo_paths: BTreeSet<String> = report
        .findings
        .iter()
        .filter_map(|finding| finding.location.as_ref())
        .map(|location| normalize_path(&location.path))
        .filter(|path| path.ends_with("Cargo.toml"))
        .collect();

    let mut out = BTreeMap::new();
    for rel in cargo_paths {
        let manifest_path = root.join(&rel);
        if !manifest_path.exists() || !manifest_path.is_file() {
            continue;
        }

        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read inline suppressions from {manifest_path}"))?;
        let parsed = parse_file_suppressions(&raw);
        if parsed.file_scoped.is_empty() && parsed.line_scoped.is_empty() {
            continue;
        }

        out.insert(rel, parsed);
    }

    Ok(out)
}

fn finding_matches_suppression(
    finding: &Finding,
    suppressions: &BTreeMap<String, FileSuppressions>,
) -> bool {
    let Some(location) = &finding.location else {
        return false;
    };
    let path = normalize_path(&location.path);
    let Some(file_rules) = suppressions.get(&path) else {
        return false;
    };

    if file_rules
        .file_scoped
        .iter()
        .any(|selector| selector_matches(selector, finding))
    {
        return true;
    }

    if let Some(line) = location.line
        && let Some(line_rules) = file_rules.line_scoped.get(&line)
        && line_rules
            .iter()
            .any(|selector| selector_matches(selector, finding))
    {
        return true;
    }

    if location.line.is_none()
        && file_rules
            .line_scoped
            .values()
            .flatten()
            .any(|selector| selector_matches(selector, finding))
    {
        return true;
    }

    false
}

fn selector_matches(selector: &SuppressionSelector, finding: &Finding) -> bool {
    match selector {
        SuppressionSelector::Any => true,
        SuppressionSelector::Check(check_id) => finding.check_id == *check_id,
        SuppressionSelector::Code(code) => finding.code == *code,
        SuppressionSelector::CheckAndCode { check_id, code } => {
            finding.check_id == *check_id && finding.code == *code
        }
    }
}

fn parse_file_suppressions(raw: &str) -> FileSuppressions {
    let mut parsed = FileSuppressions::default();

    for (idx, line) in raw.lines().enumerate() {
        let Some(hash_index) = line.find('#') else {
            continue;
        };

        let content = &line[..hash_index];
        let comment = &line[hash_index + 1..];
        let Some(selectors) = parse_comment_directive(comment) else {
            continue;
        };

        if content.trim().is_empty() {
            parsed.file_scoped.extend(selectors);
        } else {
            parsed
                .line_scoped
                .entry((idx + 1) as u32)
                .or_default()
                .extend(selectors);
        }
    }

    parsed
}

fn parse_comment_directive(comment: &str) -> Option<Vec<SuppressionSelector>> {
    const DIRECTIVE: &str = "builddiag:ignore";

    let lower = comment.to_ascii_lowercase();
    let index = lower.find(DIRECTIVE)?;
    let rest = comment[index + DIRECTIVE.len()..].trim();

    if rest.is_empty() {
        return Some(vec![SuppressionSelector::Any]);
    }

    let mut selectors: Vec<SuppressionSelector> = rest
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(parse_selector_token)
        .collect();

    if selectors.is_empty() {
        selectors.push(SuppressionSelector::Any);
    }

    Some(selectors)
}

fn parse_selector_token(token: &str) -> Option<SuppressionSelector> {
    let token = token.trim().trim_matches(|c: char| c == ';');
    if token.is_empty() {
        return None;
    }

    if token == "*" {
        return Some(SuppressionSelector::Any);
    }

    if let Some((check_id, code)) = token.split_once(':').or_else(|| token.split_once('/'))
        && !check_id.is_empty()
        && !code.is_empty()
    {
        return Some(SuppressionSelector::CheckAndCode {
            check_id: check_id.to_string(),
            code: code.to_string(),
        });
    }

    if token.contains('.') {
        return Some(SuppressionSelector::Check(token.to_string()));
    }

    Some(SuppressionSelector::Code(token.to_string()))
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
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

    #[test]
    fn inline_suppression_file_scope_by_code() {
        let temp = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"# builddiag:ignore missing_msrv
[workspace]
members = ["crates/a"]
"#,
        )
        .unwrap();

        let report = sample_report();
        let filtered = filter_report_inline_suppressions(root, &report).unwrap();
        assert_eq!(filtered.suppressed, 1);
        assert_eq!(filtered.remaining_findings, 1);
        assert_eq!(filtered.report.findings[0].code, "members_not_sorted");
        assert_eq!(filtered.report.verdict, Verdict::Warn);
    }

    #[test]
    fn inline_suppression_line_scope_matches_only_target_line() {
        let temp = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["crates/b", "crates/a"] # builddiag:ignore members_not_sorted
"#,
        )
        .unwrap();

        let report = sample_report();
        let filtered = filter_report_inline_suppressions(root, &report).unwrap();
        assert_eq!(filtered.suppressed, 1);
        assert_eq!(filtered.remaining_findings, 1);
        assert_eq!(filtered.report.findings[0].code, "missing_msrv");
        assert_eq!(filtered.report.verdict, Verdict::Fail);
    }

    #[test]
    fn inline_suppression_check_and_code_selector() {
        let temp = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"# builddiag:ignore workspace.member_ordering:members_not_sorted
[workspace]
members = ["crates/b", "crates/a"]
"#,
        )
        .unwrap();

        let report = sample_report();
        let filtered = filter_report_inline_suppressions(root, &report).unwrap();
        assert_eq!(filtered.suppressed, 1);
        assert_eq!(filtered.remaining_findings, 1);
        assert_eq!(filtered.report.findings[0].code, "missing_msrv");
    }

    #[test]
    fn inline_suppression_ignored_for_error_verdict() {
        let temp = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(root.join("Cargo.toml"), "# builddiag:ignore missing_msrv\n").unwrap();

        let mut report = sample_report();
        report.verdict = Verdict::Error;
        let filtered = filter_report_inline_suppressions(root, &report).unwrap();
        assert_eq!(filtered.suppressed, 0);
        assert_eq!(filtered.remaining_findings, 2);
        assert_eq!(filtered.report.verdict, Verdict::Error);
        assert_eq!(filtered.report.findings.len(), 2);
    }

    #[test]
    fn inline_suppression_line_selector_matches_line_less_findings() {
        let temp = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[dependencies]
serde = "*" # builddiag:ignore wildcard_version
"#,
        )
        .unwrap();

        let report = Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool: None,
            run: None,
            verdict: Verdict::Fail,
            findings: vec![Finding {
                check_id: "deps.wildcard_version".to_string(),
                code: "wildcard_version".to_string(),
                severity: Severity::Error,
                message: "wildcard".to_string(),
                location: Some(Location {
                    path: "Cargo.toml".to_string(),
                    line: None,
                    col: None,
                }),
            }],
            summary: None,
            data: None,
        };

        let filtered = filter_report_inline_suppressions(root, &report).unwrap();
        assert_eq!(filtered.suppressed, 1);
        assert_eq!(filtered.remaining_findings, 0);
        assert_eq!(filtered.report.verdict, Verdict::Pass);
    }
}
