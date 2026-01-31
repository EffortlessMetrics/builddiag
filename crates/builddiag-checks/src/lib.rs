use anyhow::{anyhow, Context, Result};
use builddiag_domain::{check_status_from_findings, parse_rust_version};
use builddiag_repo::{maybe_parse_numeric_version, RepoState};
use builddiag_types::{CheckConfig, CheckReport, CheckStatus, Config, Finding, RelationToMsrv, Severity};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;

pub struct CheckDef {
    pub id: &'static str,
    pub default_severity: Severity,
    pub default_triggers: &'static [&'static str],
}

pub const BUILTIN_CHECKS: &[CheckDef] = &[
    CheckDef {
        id: "rust.msrv_defined",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "rust.msrv_consistent",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "rust.toolchain_pinning",
        default_severity: Severity::Error,
        default_triggers: &["rust-toolchain", "rust-toolchain.toml"],
    },
    CheckDef {
        id: "rust.toolchain_msrv_relation",
        default_severity: Severity::Error,
        default_triggers: &["rust-toolchain", "rust-toolchain.toml", "Cargo.toml", "**/Cargo.toml"],
    },
    CheckDef {
        id: "tools.checksums_file_exists",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "tools.checksums_format",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "tools.checksums_coverage",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256", "scripts/tools.toml"],
    },
    CheckDef {
        id: "tools.checksums_verify_local",
        default_severity: Severity::Warn,
        default_triggers: &["scripts/tools.sha256"],
    },
    CheckDef {
        id: "workspace.resolver_v2",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml"],
    },
];

pub fn run_selected_checks(repo: &RepoState, config: &Config, allow_all: bool) -> Result<Vec<CheckReport>> {
    let overrides = config.check_overrides();
    let mut reports = Vec::new();

    for def in BUILTIN_CHECKS {
        let ov = overrides.get(def.id);
        if let Some(ov) = ov {
            if !ov.enabled {
                reports.push(CheckReport {
                    id: def.id.to_string(),
                    status: CheckStatus::Skip,
                    findings: Vec::new(),
                    skipped_reason: Some("disabled".to_string()),
                });
                continue;
            }
        }

        let triggers = effective_triggers(def, ov);
        let should_run = if allow_all {
            true
        } else {
            should_run(repo.changed_files.as_ref(), &triggers)
        };

        if !should_run {
            reports.push(CheckReport {
                id: def.id.to_string(),
                status: CheckStatus::Skip,
                findings: Vec::new(),
                skipped_reason: Some("diff-aware: no matching changed files".to_string()),
            });
            continue;
        }

        let mut report = match def.id {
            "rust.msrv_defined" => check_msrv_defined(repo, config, def.default_severity)?,
            "rust.msrv_consistent" => check_msrv_consistent(repo, config, def.default_severity)?,
            "rust.toolchain_pinning" => check_toolchain_pinning(repo, config, def.default_severity)?,
            "rust.toolchain_msrv_relation" => check_toolchain_msrv_relation(repo, config, def.default_severity)?,
            "tools.checksums_file_exists" => check_checksums_file_exists(repo, config, def.default_severity)?,
            "tools.checksums_format" => check_checksums_format(repo, config, def.default_severity)?,
            "tools.checksums_coverage" => check_checksums_coverage(repo, config, def.default_severity)?,
            "tools.checksums_verify_local" => check_checksums_verify_local(repo, config, def.default_severity)?,
            "workspace.resolver_v2" => check_workspace_resolver(repo, config, def.default_severity)?,
            _ => return Err(anyhow!("unknown check id: {}", def.id)),
        };

        // Apply severity override if configured.
        if let Some(ov) = ov {
            report.findings = apply_severity_override(report.findings, ov);
        }

        report.status = check_status_from_findings(&report.findings);
        reports.push(report);
    }

    Ok(reports)
}

fn effective_triggers(def: &CheckDef, ov: Option<&CheckConfig>) -> Vec<String> {
    if let Some(ov) = ov {
        if !ov.triggers.is_empty() {
            return ov.triggers.clone();
        }
    }
    def.default_triggers.iter().map(|s| s.to_string()).collect()
}

fn should_run(changed: Option<&BTreeSet<String>>, triggers: &[String]) -> bool {
    let Some(changed) = changed else {
        return true;
    };
    if triggers.is_empty() {
        return true;
    }

    let globset = build_globset(triggers).ok();
    let Some(globset) = globset else {
        // If triggers are invalid globs, fail open to avoid surprise skips.
        return true;
    };

    changed.iter().any(|p| globset.is_match(p))
}

fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g)?);
    }
    Ok(b.build()?)
}

fn apply_severity_override(mut findings: Vec<Finding>, ov: &CheckConfig) -> Vec<Finding> {
    for f in &mut findings {
        if f.severity != Severity::Info {
            f.severity = ov.severity;
        }
    }
    findings
}

fn mk_finding(severity: Severity, code: &str, message: impl Into<String>, path: Option<String>, line: Option<u32>) -> Finding {
    Finding {
        severity,
        code: code.to_string(),
        message: message.into(),
        path,
        line,
        column: None,
    }
}

fn check_msrv_defined(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let msrv = repo.workspace.workspace_msrv.clone();

    match config.policy.msrv.source {
        builddiag_types::MsrvSource::Workspace => {
            if msrv.is_none() && config.policy.msrv.require_defined {
                let path = repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p));
                findings.push(mk_finding(
                    default_sev,
                    "missing_msrv",
                    "Missing workspace/package rust-version (MSRV) in Cargo.toml",
                    path,
                    None,
                ));
            }
        }
        builddiag_types::MsrvSource::Any => {
            let mut any = msrv.is_some();
            if !any {
                for m in &repo.workspace.members {
                    if m.rust_version.is_some() {
                        any = true;
                        break;
                    }
                }
            }
            if !any && config.policy.msrv.require_defined {
                findings.push(mk_finding(
                    default_sev,
                    "missing_msrv",
                    "Missing rust-version (MSRV): define workspace.package.rust-version or per-crate package.rust-version",
                    repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                    None,
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "rust.msrv_defined".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_msrv_consistent(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let Some(workspace_msrv_raw) = repo.workspace.workspace_msrv.clone() else {
        return Ok(CheckReport {
            id: "rust.msrv_consistent".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no workspace/package MSRV to compare".to_string()),
        });
    };

    let workspace_msrv = match parse_rust_version(&workspace_msrv_raw) {
        Ok(v) => v.to_string(),
        Err(_) => {
            findings.push(mk_finding(
                default_sev,
                "invalid_msrv",
                format!("Invalid workspace MSRV rust-version: {workspace_msrv_raw}"),
                repo.cargo_root.as_ref().map(|p| rel_path(&repo.root, p)),
                None,
            ));
            return Ok(CheckReport {
                id: "rust.msrv_consistent".to_string(),
                status: check_status_from_findings(&findings),
                findings,
                skipped_reason: None,
            });
        }
    };

    let allowlist: HashSet<String> = config.policy.msrv.allow_overrides.iter().cloned().collect();

    for m in &repo.workspace.members {
        let rel = rel_path(&repo.root, &m.manifest_path);

        let effective = if let Some(rv) = &m.rust_version {
            Some(rv.clone())
        } else if m.rust_version_workspace {
            Some(workspace_msrv.clone())
        } else {
            None
        };

        let Some(effective_raw) = effective else {
            findings.push(mk_finding(
                default_sev,
                "missing_member_msrv",
                format!("{}: missing package.rust-version (and not set to inherit from workspace)", m.name),
                Some(rel),
                None,
            ));
            continue;
        };

        let effective_norm = match parse_rust_version(&effective_raw) {
            Ok(v) => v.to_string(),
            Err(_) => {
                findings.push(mk_finding(
                    default_sev,
                    "invalid_member_msrv",
                    format!("{}: invalid rust-version '{effective_raw}'", m.name),
                    Some(rel),
                    None,
                ));
                continue;
            }
        };

        if effective_norm != workspace_msrv {
            let allowed = config.policy.msrv.allow_per_crate_override || allowlist.contains(&rel);
            if !allowed {
                findings.push(mk_finding(
                    default_sev,
                    "msrv_mismatch",
                    format!("{}: rust-version {effective_norm} does not match workspace MSRV {workspace_msrv}", m.name),
                    Some(rel),
                    None,
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "rust.msrv_consistent".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_toolchain_pinning(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();

    let Some(tc) = &repo.toolchain else {
        if config.policy.toolchain.require_pinned {
            findings.push(mk_finding(
                default_sev,
                "missing_toolchain",
                "Missing rust-toolchain.toml (or rust-toolchain) at repo root",
                None,
                None,
            ));
        }
        return Ok(CheckReport {
            id: "rust.toolchain_pinning".to_string(),
            status: check_status_from_findings(&findings),
            findings,
            skipped_reason: None,
        });
    };

    let channel = tc.channel.trim();

    if channel.eq_ignore_ascii_case("nightly") && !config.policy.toolchain.allow_nightly {
        findings.push(mk_finding(
            default_sev,
            "nightly_disallowed",
            "Toolchain channel is 'nightly' but policy disallows nightly",
            Some(rel_path(&repo.root, &tc.path)),
            None,
        ));
    }

    if config.policy.toolchain.require_pinned {
        if channel.eq_ignore_ascii_case("stable") || channel.eq_ignore_ascii_case("beta") || channel.eq_ignore_ascii_case("nightly") {
            findings.push(mk_finding(
                default_sev,
                "unpinned_channel",
                format!("Toolchain channel '{channel}' is not pinned to a specific version"),
                Some(rel_path(&repo.root, &tc.path)),
                None,
            ));
        } else if maybe_parse_numeric_version(channel)?.is_none() {
            findings.push(mk_finding(
                default_sev,
                "invalid_toolchain_version",
                format!("Toolchain channel '{channel}' is not a valid numeric Rust version"),
                Some(rel_path(&repo.root, &tc.path)),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: "rust.toolchain_pinning".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_toolchain_msrv_relation(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();
    let Some(tc) = &repo.toolchain else {
        return Ok(CheckReport {
            id: "rust.toolchain_msrv_relation".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no toolchain file".to_string()),
        });
    };
    let Some(msrv_raw) = &repo.workspace.workspace_msrv else {
        return Ok(CheckReport {
            id: "rust.toolchain_msrv_relation".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no workspace/package MSRV".to_string()),
        });
    };

    let toolchain_ver = match maybe_parse_numeric_version(&tc.channel)? {
        Some(v) => v,
        None => {
            return Ok(CheckReport {
                id: "rust.toolchain_msrv_relation".to_string(),
                status: CheckStatus::Skip,
                findings: Vec::new(),
                skipped_reason: Some("non-numeric toolchain channel".to_string()),
            })
        }
    };

    let msrv_ver = parse_rust_version(msrv_raw)
        .map_err(|e| anyhow!("invalid MSRV '{msrv_raw}': {e}"))?
        .to_string();

    let tc_v = parse_rust_version(&toolchain_ver)?;
    let ms_v = parse_rust_version(&msrv_ver)?;

    let ok = match config.policy.toolchain.relation_to_msrv {
        RelationToMsrv::Equals => tc_v == ms_v,
        RelationToMsrv::AtLeast => tc_v >= ms_v,
    };

    if !ok {
        let relation = match config.policy.toolchain.relation_to_msrv {
            RelationToMsrv::Equals => "must equal",
            RelationToMsrv::AtLeast => "must be at least",
        };
        findings.push(mk_finding(
            default_sev,
            "toolchain_msrv_mismatch",
            format!("Toolchain ({}) {relation} MSRV ({})", tc_v, ms_v),
            Some(rel_path(&repo.root, &tc.path)),
            None,
        ));
    }

    Ok(CheckReport {
        id: "rust.toolchain_msrv_relation".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_file_exists(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();
    if repo.tools_checksums.is_none() && config.policy.checksums.require_file {
        findings.push(mk_finding(
            default_sev,
            "missing_checksums",
            "Missing scripts/tools.sha256",
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    Ok(CheckReport {
        id: "tools.checksums_file_exists".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_format(repo: &RepoState, _config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_format".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let mut findings = Vec::new();
    let mut seen_paths = HashSet::new();

    for e in &cks.entries {
        let rel = rel_path(&repo.root, &cks.path);

        // Hash must be 64 hex chars.
        if e.hash.len() != 64 || hex::decode(&e.hash).is_err() {
            findings.push(mk_finding(
                default_sev,
                "invalid_hash",
                format!("Invalid sha256 hash for path '{}': '{}'", e.path, e.hash),
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        }

        if e.path.trim().is_empty() {
            findings.push(mk_finding(
                default_sev,
                "missing_path",
                "Checksum line missing path",
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        } else {
            if !seen_paths.insert(e.path.clone()) {
                findings.push(mk_finding(
                    default_sev,
                    "duplicate_path",
                    format!("Duplicate checksum entry for path '{}'", e.path),
                    Some(rel.clone()),
                    Some(e.line as u32),
                ));
            }
        }
    }

    Ok(CheckReport {
        id: "tools.checksums_format".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_coverage(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    if !config.policy.checksums.require_coverage {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("coverage not required by policy".to_string()),
        });
    }

    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let Some((_manifest_path, manifest)) = &repo.tools_manifest else {
        return Ok(CheckReport {
            id: "tools.checksums_coverage".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no tools manifest".to_string()),
        });
    };

    let have: HashSet<String> = cks.entries.iter().map(|e| e.path.clone()).collect();
    let mut expected = HashSet::new();
    for t in &manifest.tool {
        for f in &t.files {
            expected.insert(f.clone());
        }
    }

    let mut findings = Vec::new();

    for f in expected.difference(&have) {
        findings.push(mk_finding(
            default_sev,
            "missing_checksum",
            format!("Missing checksum entry for expected tool file '{}'", f),
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    // extras are usually benign; mark warn by default, still overrideable.
    for f in have.difference(&expected) {
        findings.push(mk_finding(
            Severity::Warn,
            "unexpected_checksum",
            format!("Checksum contains entry not present in tools manifest: '{}'", f),
            Some(config.paths.tools_checksums.clone()),
            None,
        ));
    }

    Ok(CheckReport {
        id: "tools.checksums_coverage".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_verify_local(repo: &RepoState, config: &Config, default_sev: Severity) -> Result<CheckReport> {
    if !config.policy.checksums.verify_local_files {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("local verification not enabled".to_string()),
        });
    }
    let Some(cks) = &repo.tools_checksums else {
        return Ok(CheckReport {
            id: "tools.checksums_verify_local".to_string(),
            status: CheckStatus::Skip,
            findings: Vec::new(),
            skipped_reason: Some("no checksums file".to_string()),
        });
    };

    let mut findings = Vec::new();

    for e in &cks.entries {
        let p = repo.root.join(&e.path);
        if !p.exists() {
            findings.push(mk_finding(
                Severity::Warn,
                "missing_tool_file",
                format!("Tool file '{}' not found on disk (skipping hash verify)", e.path),
                Some(e.path.clone()),
                None,
            ));
            continue;
        }

        let bytes = fs::read(&p).with_context(|| format!("read {}", p))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let got = format!("{:x}", hasher.finalize());
        let want = e.hash.to_ascii_lowercase();

        if got != want {
            findings.push(mk_finding(
                default_sev,
                "hash_mismatch",
                format!("Hash mismatch for '{}': expected {}, got {}", e.path, want, got),
                Some(e.path.clone()),
                None,
            ));
        }
    }

    Ok(CheckReport {
        id: "tools.checksums_verify_local".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_workspace_resolver(repo: &RepoState, _config: &Config, default_sev: Severity) -> Result<CheckReport> {
    let mut findings = Vec::new();
    if !repo.workspace.is_workspace {
        return Ok(CheckReport {
            id: "workspace.resolver_v2".to_string(),
            status: CheckStatus::Skip,
            findings,
            skipped_reason: Some("not a workspace".to_string()),
        });
    }

    let resolver = repo.workspace.workspace_resolver.as_deref();
    if resolver != Some("2") {
        findings.push(mk_finding(
            default_sev,
            "resolver_not_v2",
            format!("workspace.resolver is {:?}; expected '2'", resolver),
            Some("Cargo.toml".to_string()),
            None,
        ));
    }

    Ok(CheckReport {
        id: "workspace.resolver_v2".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn rel_path(root: &camino::Utf8Path, p: &camino::Utf8Path) -> String {
    p.strip_prefix(root).ok().unwrap_or(p).to_string()
}
