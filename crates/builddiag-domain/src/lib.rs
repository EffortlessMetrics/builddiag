use anyhow::{anyhow, Result};
use builddiag_types::{CheckReport, CheckStatus, FailOn, Finding, Severity, Summary, SummaryCounts, Verdict};
use semver::Version;

/// Parse a Rust toolchain or MSRV version string.
///
/// Accepts `1.75`, `1.75.0` and normalizes to a full semver.
pub fn parse_rust_version(input: &str) -> Result<Version> {
    let s = input.trim();
    if s.is_empty() {
        return Err(anyhow!("empty version"));
    }

    // Semver expects three components.
    let parts: Vec<&str> = s.split('.').collect();
    let normalized = match parts.len() {
        1 => format!("{}.0.0", s),
        2 => format!("{}.0", s),
        _ => s.to_string(),
    };

    Version::parse(&normalized).map_err(|e| anyhow!("invalid version '{s}': {e}"))
}

pub fn check_status_from_findings(findings: &[Finding]) -> CheckStatus {
    if findings.iter().any(|f| f.severity == Severity::Error) {
        CheckStatus::Fail
    } else if findings.iter().any(|f| f.severity == Severity::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    }
}

pub fn summarize(checks: &[CheckReport]) -> Summary {
    let mut counts = SummaryCounts {
        info: 0,
        warn: 0,
        error: 0,
    };

    for c in checks {
        for f in &c.findings {
            match f.severity {
                Severity::Info => counts.info += 1,
                Severity::Warn => counts.warn += 1,
                Severity::Error => counts.error += 1,
            }
        }
    }

    let mut verdict = Verdict::Pass;
    let mut any_ran = false;

    for c in checks {
        match c.status {
            CheckStatus::Skip => {}
            CheckStatus::Pass => any_ran = true,
            CheckStatus::Warn => {
                any_ran = true;
                if verdict != Verdict::Fail {
                    verdict = Verdict::Warn;
                }
            }
            CheckStatus::Fail => {
                any_ran = true;
                verdict = Verdict::Fail;
            }
        }
    }

    if !any_ran {
        verdict = Verdict::Skip;
    }

    let mut reasons = Vec::new();
    for c in checks {
        match c.status {
            CheckStatus::Fail => reasons.push(format!("{}: fail", c.id)),
            CheckStatus::Warn => reasons.push(format!("{}: warn", c.id)),
            _ => {}
        }
    }

    Summary {
        counts,
        verdict,
        reasons,
    }
}

/// Decide the process exit code for the report based on fail policy.
///
/// Contract:
/// - 0: pass (or skip)
/// - 2: fail
/// - 3: warn-as-fail
pub fn exit_code_for(summary: &Summary, fail_on: FailOn) -> i32 {
    match summary.verdict {
        Verdict::Fail => 2,
        Verdict::Warn => match fail_on {
            FailOn::Warn => 3,
            FailOn::Error | FailOn::Never => 0,
        },
        Verdict::Pass | Verdict::Skip => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{Finding, Severity};

    #[test]
    fn parse_versions() {
        assert_eq!(parse_rust_version("1").unwrap().to_string(), "1.0.0");
        assert_eq!(parse_rust_version("1.75").unwrap().to_string(), "1.75.0");
        assert_eq!(parse_rust_version("1.75.0").unwrap().to_string(), "1.75.0");
        assert!(parse_rust_version("").is_err());
    }

    #[test]
    fn status_from_findings() {
        let ok: Vec<Finding> = Vec::new();
        assert_eq!(check_status_from_findings(&ok), CheckStatus::Pass);

        let warn = vec![Finding {
            severity: Severity::Warn,
            code: "x".into(),
            message: "x".into(),
            path: None,
            line: None,
            column: None,
        }];
        assert_eq!(check_status_from_findings(&warn), CheckStatus::Warn);

        let err = vec![Finding {
            severity: Severity::Error,
            code: "x".into(),
            message: "x".into(),
            path: None,
            line: None,
            column: None,
        }];
        assert_eq!(check_status_from_findings(&err), CheckStatus::Fail);
    }
}
