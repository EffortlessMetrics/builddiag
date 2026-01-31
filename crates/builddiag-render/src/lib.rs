use builddiag_types::{CheckStatus, Finding, Report, Severity, Verdict};

pub fn render_markdown(report: &Report) -> String {
    let mut out = String::new();

    let icon = match report.summary.verdict {
        Verdict::Pass => "✅",
        Verdict::Warn => "⚠️",
        Verdict::Fail => "❌",
        Verdict::Skip => "⏭️",
    };

    out.push_str(&format!(
        "## builddiag: {} {:?} ({} errors, {} warnings)\n\n",
        icon,
        report.summary.verdict,
        report.summary.counts.error,
        report.summary.counts.warn
    ));

    let mut rows = Vec::new();
    for c in &report.checks {
        if c.status == CheckStatus::Pass || c.status == CheckStatus::Skip {
            continue;
        }
        for f in &c.findings {
            rows.push((c.id.as_str(), f));
        }
    }

    // sort: severity desc, then check id, then path
    rows.sort_by(|a, b| {
        let sa = severity_rank(a.1.severity);
        let sb = severity_rank(b.1.severity);
        sb.cmp(&sa)
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| a.1.path.as_deref().unwrap_or("").cmp(b.1.path.as_deref().unwrap_or("")))
            .then_with(|| a.1.line.unwrap_or(0).cmp(&b.1.line.unwrap_or(0)))
    });

    if rows.is_empty() {
        out.push_str("No findings.\n");
        return out;
    }

    out.push_str("| severity | check | location | message |\n");
    out.push_str("|---|---|---|---|\n");

    for (check_id, f) in rows {
        let sev = match f.severity {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        };
        let loc = format_location(f);
        let msg = escape_md(&f.message);
        out.push_str(&format!("| {} | {} | {} | {} |\n", sev, check_id, loc, msg));
    }

    out.push_str("\nReproduce:\n");
    out.push_str("`builddiag check --root .`\n");

    out
}

pub fn render_github_annotations(report: &Report) -> Vec<String> {
    let mut lines = Vec::new();
    for c in &report.checks {
        for f in &c.findings {
            if f.path.is_none() || f.line.is_none() {
                continue;
            }
            let kind = match f.severity {
                Severity::Error => "error",
                Severity::Warn => "warning",
                Severity::Info => "notice",
            };
            let mut s = format!(
                "::{} file={},line={}::[{}:{}] {}",
                kind,
                f.path.as_deref().unwrap_or(""),
                f.line.unwrap_or(1),
                c.id,
                f.code,
                f.message
            );
            if let Some(col) = f.column {
                s = format!(
                    "::{} file={},line={},col={}::[{}:{}] {}",
                    kind,
                    f.path.as_deref().unwrap_or(""),
                    f.line.unwrap_or(1),
                    col,
                    c.id,
                    f.code,
                    f.message
                );
            }
            lines.push(s);
        }
    }
    lines
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Error => 3,
        Severity::Warn => 2,
        Severity::Info => 1,
    }
}

fn format_location(f: &Finding) -> String {
    match (&f.path, f.line) {
        (Some(p), Some(l)) => format!("{}:{}", escape_md(p), l),
        (Some(p), None) => escape_md(p),
        (None, Some(l)) => format!("line {}", l),
        (None, None) => "".to_string(),
    }
}

fn escape_md(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::*;
    use chrono::Utc;

    #[test]
    fn markdown_smoke() {
        let report = Report {
            schema: SchemaId("builddiag.report.v1".to_string()),
            tool: ToolInfo { name: "builddiag".into(), version: "0.1.0".into() },
            run: RunInfo { id: "x".into(), started_at: Utc::now(), ended_at: None },
            repo: RepoInfo { root: ".".into(), detected: RepoDetected { is_workspace: true, members: 1 } },
            inputs: Inputs { cargo_root: Some("Cargo.toml".into()), rust_toolchain: None, tools_checksums: None, tools_manifest: None },
            checks: vec![CheckReport {
                id: "rust.msrv_defined".into(),
                status: CheckStatus::Fail,
                findings: vec![Finding { severity: Severity::Error, code: "missing".into(), message: "Missing MSRV".into(), path: Some("Cargo.toml".into()), line: Some(1), column: None }],
                skipped_reason: None,
            }],
            summary: Summary { counts: SummaryCounts { info: 0, warn: 0, error: 1 }, verdict: Verdict::Fail, reasons: vec!["rust.msrv_defined: fail".into()] },
        };
        let md = render_markdown(&report);
        assert!(md.contains("Missing MSRV"));
        let ann = render_github_annotations(&report);
        assert!(!ann.is_empty());
    }
}
