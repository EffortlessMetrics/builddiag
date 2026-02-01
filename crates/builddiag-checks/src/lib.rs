use anyhow::{Context, Result, anyhow};
use builddiag_domain::{check_status_from_findings, parse_rust_version};
use builddiag_repo::{RepoState, maybe_parse_numeric_version};
use builddiag_types::{
    CheckConfig, CheckReport, CheckStatus, Config, Finding, ProfileCheckState, RelationToMsrv,
    Severity,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;

/// Documentation for a check, used by the `explain` subcommand.
#[derive(Debug, Clone)]
pub struct CheckDocumentation {
    /// Check ID (e.g., "rust.msrv_defined").
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Detailed description of what the check validates.
    pub description: &'static str,
    /// Short help text for remediation.
    pub help: &'static str,
    /// Optional documentation URL.
    pub url: Option<&'static str>,
    /// Finding codes this check can produce.
    pub codes: &'static [&'static str],
}

/// Registry of check documentation.
pub static CHECK_DOCS: &[CheckDocumentation] = &[
    CheckDocumentation {
        id: "rust.msrv_defined",
        name: "MSRV Defined",
        description: "Validates that the Minimum Supported Rust Version (MSRV) is explicitly \
                      defined in Cargo.toml. MSRV helps users and CI systems know which Rust \
                      version is required to build your crate.",
        help: "Add `rust-version = \"1.XX.0\"` to your workspace Cargo.toml under \
               [workspace.package] or [package].",
        url: Some("https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field"),
        codes: &["missing_msrv"],
    },
    CheckDocumentation {
        id: "rust.msrv_consistent",
        name: "MSRV Consistent",
        description: "Validates that all workspace members have consistent MSRV values. \
                      Inconsistent MSRV across crates can cause confusing build failures.",
        help: "Ensure all crates either inherit from workspace.package.rust-version or \
               explicitly set the same rust-version.",
        url: Some("https://doc.rust-lang.org/cargo/reference/workspaces.html"),
        codes: &[
            "invalid_msrv",
            "missing_member_msrv",
            "invalid_member_msrv",
            "msrv_mismatch",
        ],
    },
    CheckDocumentation {
        id: "rust.toolchain_pinning",
        name: "Toolchain Pinning",
        description: "Validates that rust-toolchain.toml pins the Rust version to a specific \
                      release (e.g., \"1.75.0\") rather than a moving target like \"stable\".",
        help: "Create rust-toolchain.toml with `channel = \"1.XX.0\"` to pin the version.",
        url: Some("https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file"),
        codes: &[
            "missing_toolchain",
            "nightly_disallowed",
            "unpinned_channel",
            "invalid_toolchain_version",
        ],
    },
    CheckDocumentation {
        id: "rust.toolchain_msrv_relation",
        name: "Toolchain-MSRV Relation",
        description: "Validates that the pinned toolchain version matches or exceeds the MSRV. \
                      This ensures CI tests against the version users will actually use.",
        help: "Set your toolchain channel to match your MSRV, or configure \
               policy.toolchain.relation_to_msrv = \"at_least\" to allow newer toolchains.",
        url: None,
        codes: &["toolchain_msrv_mismatch"],
    },
    CheckDocumentation {
        id: "tools.checksums_file_exists",
        name: "Checksums File Exists",
        description: "Validates that the tools checksums file (scripts/tools.sha256) exists. \
                      This file contains SHA256 hashes for tool binaries to verify integrity.",
        help: "Create scripts/tools.sha256 with checksums in the format: \
               `<sha256hash>  <filepath>`",
        url: None,
        codes: &["missing_checksums"],
    },
    CheckDocumentation {
        id: "tools.checksums_format",
        name: "Checksums Format",
        description: "Validates that the checksums file has valid format: 64-character hex \
                      SHA256 hashes followed by file paths, no duplicates.",
        help: "Ensure each line follows the format: `<64-char-sha256>  <filepath>`. \
               Generate with: `sha256sum <file>`",
        url: None,
        codes: &["invalid_hash", "missing_path", "duplicate_path"],
    },
    CheckDocumentation {
        id: "tools.checksums_coverage",
        name: "Checksums Coverage",
        description: "Validates that all tool files listed in the tools manifest have \
                      corresponding checksum entries.",
        help: "Add missing checksums for all files listed in scripts/tools.toml.",
        url: None,
        codes: &["missing_checksum", "unexpected_checksum"],
    },
    CheckDocumentation {
        id: "tools.checksums_verify_local",
        name: "Checksums Verify Local",
        description: "Verifies that local tool files match their recorded checksums. \
                      Detects tampering or corruption of tool binaries.",
        help: "Re-download or regenerate tools with mismatched checksums, then update \
               scripts/tools.sha256.",
        url: None,
        codes: &["missing_tool_file", "hash_mismatch"],
    },
    CheckDocumentation {
        id: "workspace.resolver_v2",
        name: "Workspace Resolver v2",
        description: "Validates that Cargo workspaces use resolver version 2. Resolver v2 \
                      has better feature unification and is required for edition 2021+.",
        help: "Add `resolver = \"2\"` to your [workspace] section in Cargo.toml.",
        url: Some(
            "https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2",
        ),
        codes: &["resolver_not_v2"],
    },
];

/// Look up documentation for a check ID or finding code.
///
/// Returns `Some(CheckDocumentation)` if found, `None` otherwise.
pub fn explain_check(check_or_code: &str) -> Option<&'static CheckDocumentation> {
    // First try exact match on check ID
    if let Some(doc) = CHECK_DOCS.iter().find(|d| d.id == check_or_code) {
        return Some(doc);
    }

    // Then try finding by code
    CHECK_DOCS.iter().find(|d| d.codes.contains(&check_or_code))
}

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
        default_triggers: &[
            "rust-toolchain",
            "rust-toolchain.toml",
            "Cargo.toml",
            "**/Cargo.toml",
        ],
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

pub fn run_selected_checks(
    repo: &RepoState,
    config: &Config,
    allow_all: bool,
) -> Result<Vec<CheckReport>> {
    let overrides = config.check_overrides();
    let profile = config.profile;
    let mut reports = Vec::new();

    for def in BUILTIN_CHECKS {
        let ov = overrides.get(def.id);

        // Determine effective severity from profile, then user override
        let profile_state = profile.check_state(def.id);

        // User override takes precedence over profile
        let (enabled, effective_severity) = if let Some(ov) = ov {
            (ov.enabled, ov.severity)
        } else {
            match profile_state {
                ProfileCheckState::Enabled(sev) => (true, sev),
                ProfileCheckState::Skip => (false, def.default_severity),
            }
        };

        if !enabled {
            reports.push(CheckReport {
                id: def.id.to_string(),
                status: CheckStatus::Skip,
                findings: Vec::new(),
                skipped_reason: Some(if ov.is_some() {
                    "disabled".to_string()
                } else {
                    format!("disabled by {} profile", profile)
                }),
            });
            continue;
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
            "rust.msrv_defined" => check_msrv_defined(repo, config, effective_severity)?,
            "rust.msrv_consistent" => check_msrv_consistent(repo, config, effective_severity)?,
            "rust.toolchain_pinning" => check_toolchain_pinning(repo, config, effective_severity)?,
            "rust.toolchain_msrv_relation" => {
                check_toolchain_msrv_relation(repo, config, effective_severity)?
            }
            "tools.checksums_file_exists" => {
                check_checksums_file_exists(repo, config, effective_severity)?
            }
            "tools.checksums_format" => check_checksums_format(repo, config, effective_severity)?,
            "tools.checksums_coverage" => {
                check_checksums_coverage(repo, config, effective_severity)?
            }
            "tools.checksums_verify_local" => {
                check_checksums_verify_local(repo, config, effective_severity)?
            }
            "workspace.resolver_v2" => check_workspace_resolver(repo, config, effective_severity)?,
            _ => return Err(anyhow!("unknown check id: {}", def.id)),
        };

        // Apply severity override if explicitly configured (belt-and-suspenders with profile)
        if let Some(ov) = ov {
            report.findings = apply_severity_override(report.findings, ov);
        }

        report.status = check_status_from_findings(&report.findings);
        reports.push(report);
    }

    Ok(reports)
}

fn effective_triggers(def: &CheckDef, ov: Option<&CheckConfig>) -> Vec<String> {
    if let Some(ov) = ov
        && !ov.triggers.is_empty()
    {
        return ov.triggers.clone();
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

fn mk_finding(
    severity: Severity,
    code: &str,
    message: impl Into<String>,
    path: Option<String>,
    line: Option<u32>,
) -> Finding {
    Finding {
        severity,
        code: code.to_string(),
        message: message.into(),
        path,
        line,
        column: None,
    }
}

fn check_msrv_defined(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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

fn check_msrv_consistent(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
                format!(
                    "{}: missing package.rust-version (and not set to inherit from workspace)",
                    m.name
                ),
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

fn check_toolchain_pinning(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
        if channel.eq_ignore_ascii_case("stable")
            || channel.eq_ignore_ascii_case("beta")
            || channel.eq_ignore_ascii_case("nightly")
        {
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

fn check_toolchain_msrv_relation(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
            });
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

fn check_checksums_file_exists(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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

fn check_checksums_format(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
        } else if !seen_paths.insert(e.path.clone()) {
            findings.push(mk_finding(
                default_sev,
                "duplicate_path",
                format!("Duplicate checksum entry for path '{}'", e.path),
                Some(rel.clone()),
                Some(e.line as u32),
            ));
        }
    }

    Ok(CheckReport {
        id: "tools.checksums_format".to_string(),
        status: check_status_from_findings(&findings),
        findings,
        skipped_reason: None,
    })
}

fn check_checksums_coverage(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
            format!(
                "Checksum contains entry not present in tools manifest: '{}'",
                f
            ),
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

fn check_checksums_verify_local(
    repo: &RepoState,
    config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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
                format!(
                    "Tool file '{}' not found on disk (skipping hash verify)",
                    e.path
                ),
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
                format!(
                    "Hash mismatch for '{}': expected {}, got {}",
                    e.path, want, got
                ),
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

fn check_workspace_resolver(
    repo: &RepoState,
    _config: &Config,
    default_sev: Severity,
) -> Result<CheckReport> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_repo::{Member, RepoState, Toolchain, WorkspaceInfo};
    use builddiag_types::{Config, MsrvSource, RelationToMsrv, Severity};
    use camino::Utf8PathBuf;

    /// Helper to create a minimal RepoState for testing
    fn mock_repo_state() -> RepoState {
        RepoState {
            root: Utf8PathBuf::from("/test/repo"),
            cargo_root: Some(Utf8PathBuf::from("/test/repo/Cargo.toml")),
            toolchain: None,
            workspace: WorkspaceInfo {
                is_workspace: true,
                members: Vec::new(),
                workspace_msrv: None,
                workspace_edition: Some("2021".to_string()),
                workspace_resolver: Some("2".to_string()),
            },
            tools_checksums: None,
            tools_manifest: None,
            changed_files: None,
        }
    }

    /// Helper to create a RepoState with workspace MSRV set
    fn mock_repo_with_msrv(msrv: &str) -> RepoState {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_msrv = Some(msrv.to_string());
        repo
    }

    /// Helper to create a RepoState with toolchain
    fn mock_repo_with_toolchain(channel: &str) -> RepoState {
        let mut repo = mock_repo_state();
        repo.toolchain = Some(Toolchain {
            path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
            channel: channel.to_string(),
        });
        repo
    }

    /// Helper to create a RepoState with both MSRV and toolchain
    fn mock_repo_with_msrv_and_toolchain(msrv: &str, channel: &str) -> RepoState {
        let mut repo = mock_repo_with_msrv(msrv);
        repo.toolchain = Some(Toolchain {
            path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
            channel: channel.to_string(),
        });
        repo
    }

    /// Helper to create a RepoState with workspace members
    fn mock_repo_with_members(workspace_msrv: Option<&str>, members: Vec<Member>) -> RepoState {
        let mut repo = mock_repo_state();
        repo.workspace.workspace_msrv = workspace_msrv.map(|s| s.to_string());
        repo.workspace.members = members;
        repo
    }

    /// Helper to create a Member
    fn mock_member(name: &str, rust_version: Option<&str>, rust_version_workspace: bool) -> Member {
        Member {
            name: name.to_string(),
            manifest_path: Utf8PathBuf::from(format!("/test/repo/crates/{}/Cargo.toml", name)),
            rust_version: rust_version.map(|s| s.to_string()),
            rust_version_workspace,
            edition: Some("2021".to_string()),
            edition_workspace: true,
        }
    }

    // =========================================================================
    // Task 6.1: Tests for check_msrv_defined
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn msrv_defined_passes_when_workspace_msrv_is_set() {
        // Arrange: RepoState with workspace_msrv set
        let repo = mock_repo_with_msrv("1.70.0");
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Pass, no findings
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_fails_when_workspace_msrv_is_none_and_require_defined_is_true() {
        // Arrange: RepoState without workspace_msrv, require_defined = true (default)
        let repo = mock_repo_state();
        let config = Config::default();

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Fail, finding has message about missing MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
        assert!(report.findings[0].message.contains("Missing"));
    }

    #[test]
    fn msrv_defined_passes_when_require_defined_is_false() {
        // Arrange: RepoState without workspace_msrv, but require_defined = false
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.msrv.require_defined = false;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: status is Pass (no requirement to have MSRV)
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_with_any_source_passes_when_member_has_msrv() {
        // Arrange: No workspace MSRV, but a member has rust_version, source = Any
        let members = vec![mock_member("my-crate", Some("1.70.0"), false)];
        let repo = mock_repo_with_members(None, members);
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass because at least one crate has MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_defined_with_any_source_fails_when_no_msrv_anywhere() {
        // Arrange: No workspace MSRV, no member MSRV, source = Any
        let members = vec![mock_member("my-crate", None, false)];
        let repo = mock_repo_with_members(None, members);
        let mut config = Config::default();
        config.policy.msrv.source = MsrvSource::Any;

        // Act
        let report = check_msrv_defined(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail because no MSRV defined anywhere
        assert_eq!(report.status, CheckStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "missing_msrv");
    }

    // =========================================================================
    // Task 6.2: Tests for check_msrv_consistent
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn msrv_consistent_passes_when_all_members_match_workspace_msrv() {
        // Arrange: Workspace MSRV = 1.70.0, all members inherit from workspace
        let members = vec![
            mock_member("crate-a", None, true), // inherits from workspace
            mock_member("crate-b", None, true), // inherits from workspace
        ];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, all members consistent
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn msrv_consistent_fails_when_member_has_different_msrv() {
        // Arrange: Workspace MSRV = 1.70.0, one member has different MSRV
        let members = vec![
            mock_member("crate-a", None, true),            // inherits 1.70.0
            mock_member("crate-b", Some("1.65.0"), false), // different MSRV
        ];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, member has mismatched MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "msrv_mismatch"));
    }

    #[test]
    fn msrv_consistent_skips_when_no_workspace_msrv() {
        // Arrange: No workspace MSRV to compare against
        let members = vec![mock_member("crate-a", Some("1.70.0"), false)];
        let repo = mock_repo_with_members(None, members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no workspace MSRV to compare
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no workspace"));
    }

    #[test]
    fn msrv_consistent_fails_when_member_missing_msrv_and_not_inheriting() {
        // Arrange: Workspace MSRV set, member has no MSRV and doesn't inherit
        let members = vec![mock_member("crate-a", None, false)]; // no MSRV, not inheriting
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let config = Config::default();

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, member missing MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_member_msrv")
        );
    }

    #[test]
    fn msrv_consistent_passes_with_allow_per_crate_override() {
        // Arrange: Member has different MSRV but allow_per_crate_override is true
        let members = vec![mock_member("crate-a", Some("1.65.0"), false)];
        let repo = mock_repo_with_members(Some("1.70.0"), members);
        let mut config = Config::default();
        config.policy.msrv.allow_per_crate_override = true;

        // Act
        let report = check_msrv_consistent(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, override allowed
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    // =========================================================================
    // Task 6.3: Tests for check_toolchain_pinning
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn toolchain_pinning_passes_when_pinned_to_specific_version() {
        // Arrange: Toolchain pinned to specific version like "1.75.0"
        let repo = mock_repo_with_toolchain("1.75.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain is pinned
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_stable() {
        // Arrange: Toolchain is "stable" (unpinned)
        let repo = mock_repo_with_toolchain("stable");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "stable" is not pinned
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_beta() {
        // Arrange: Toolchain is "beta" (unpinned)
        let repo = mock_repo_with_toolchain("beta");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "beta" is not pinned
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    #[test]
    fn toolchain_pinning_fails_when_channel_is_nightly() {
        // Arrange: Toolchain is "nightly" (unpinned and disallowed by default)
        let repo = mock_repo_with_toolchain("nightly");
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, "nightly" is not pinned and disallowed
        assert_eq!(report.status, CheckStatus::Fail);
        // Should have both nightly_disallowed and unpinned_channel findings
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "nightly_disallowed" || f.code == "unpinned_channel")
        );
    }

    #[test]
    fn toolchain_pinning_fails_when_missing_toolchain_file_and_required() {
        // Arrange: No toolchain file, require_pinned = true (default)
        let repo = mock_repo_state(); // no toolchain
        let config = Config::default();

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, missing toolchain file
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_toolchain")
        );
    }

    #[test]
    fn toolchain_pinning_passes_when_missing_toolchain_file_and_not_required() {
        // Arrange: No toolchain file, require_pinned = false
        let repo = mock_repo_state();
        let mut config = Config::default();
        config.policy.toolchain.require_pinned = false;

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain not required
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_pinning_allows_nightly_when_configured() {
        // Arrange: Toolchain is "nightly" but allow_nightly = true
        let repo = mock_repo_with_toolchain("nightly");
        let mut config = Config::default();
        config.policy.toolchain.allow_nightly = true;

        // Act
        let report = check_toolchain_pinning(&repo, &config, Severity::Error).unwrap();

        // Assert: Still fails for unpinned, but no nightly_disallowed finding
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "nightly_disallowed")
        );
        // But should still have unpinned_channel
        assert!(report.findings.iter().any(|f| f.code == "unpinned_channel"));
    }

    // =========================================================================
    // Task 6.4: Tests for check_toolchain_msrv_relation
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn toolchain_msrv_relation_passes_when_toolchain_equals_msrv() {
        // Arrange: Toolchain = 1.70.0, MSRV = 1.70.0, relation = Equals (default)
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain equals MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_msrv_relation_fails_when_toolchain_less_than_msrv() {
        // Arrange: Toolchain = 1.65.0, MSRV = 1.70.0, relation = Equals
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.65.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, toolchain doesn't equal MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "toolchain_msrv_mismatch")
        );
    }

    #[test]
    fn toolchain_msrv_relation_fails_when_toolchain_greater_than_msrv_with_equals() {
        // Arrange: Toolchain = 1.75.0, MSRV = 1.70.0, relation = Equals
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.75.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Fail, toolchain doesn't equal MSRV
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "toolchain_msrv_mismatch")
        );
    }

    #[test]
    fn toolchain_msrv_relation_passes_when_toolchain_at_least_msrv() {
        // Arrange: Toolchain = 1.75.0, MSRV = 1.70.0, relation = AtLeast
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "1.75.0");
        let mut config = Config::default();
        config.policy.toolchain.relation_to_msrv = RelationToMsrv::AtLeast;

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Pass, toolchain >= MSRV
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_non_numeric_toolchain_channel() {
        // Arrange: Toolchain = "stable" (non-numeric)
        let repo = mock_repo_with_msrv_and_toolchain("1.70.0", "stable");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, can't compare non-numeric channel
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("non-numeric"));
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_no_toolchain_file() {
        // Arrange: No toolchain file
        let repo = mock_repo_with_msrv("1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no toolchain file
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no toolchain"));
    }

    #[test]
    fn toolchain_msrv_relation_skips_when_no_workspace_msrv() {
        // Arrange: Toolchain set but no MSRV
        let repo = mock_repo_with_toolchain("1.70.0");
        let config = Config::default();

        // Act
        let report = check_toolchain_msrv_relation(&repo, &config, Severity::Error).unwrap();

        // Assert: Skip, no MSRV to compare
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("no workspace"));
    }

    // =========================================================================
    // Task 6.5: Tests for check_workspace_resolver
    // Requirements: 5.1, 5.2
    // =========================================================================

    #[test]
    fn workspace_resolver_passes_when_resolver_is_2() {
        // Arrange: Workspace with resolver = "2"
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = Some("2".to_string());
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Pass, resolver is "2"
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn workspace_resolver_fails_when_resolver_is_missing() {
        // Arrange: Workspace with no resolver set
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = None;
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Fail, resolver not set to "2"
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.findings.iter().any(|f| f.code == "resolver_not_v2"));
    }

    #[test]
    fn workspace_resolver_fails_when_resolver_is_not_2() {
        // Arrange: Workspace with resolver = "1"
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = true;
        repo.workspace.workspace_resolver = Some("1".to_string());
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Fail, resolver is not "2"
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.findings.iter().any(|f| f.code == "resolver_not_v2"));
    }

    #[test]
    fn workspace_resolver_skips_when_not_a_workspace() {
        // Arrange: Not a workspace (single crate)
        let mut repo = mock_repo_state();
        repo.workspace.is_workspace = false;
        let config = Config::default();

        // Act
        let report = check_workspace_resolver(&repo, &config, Severity::Warn).unwrap();

        // Assert: Skip, not a workspace
        assert_eq!(report.status, CheckStatus::Skip);
        assert!(report.skipped_reason.is_some());
        assert!(report.skipped_reason.unwrap().contains("not a workspace"));
    }

    // =========================================================================
    // Contract Tests: Ensure finding codes and check documentation are in sync
    // =========================================================================

    #[test]
    fn contract_every_builtin_check_has_documentation() {
        // Every check ID in BUILTIN_CHECKS must have a corresponding entry in CHECK_DOCS
        for def in super::BUILTIN_CHECKS {
            let doc = super::CHECK_DOCS.iter().find(|d| d.id == def.id);
            assert!(
                doc.is_some(),
                "Check '{}' is in BUILTIN_CHECKS but missing from CHECK_DOCS",
                def.id
            );
        }
    }

    #[test]
    fn contract_every_documented_check_is_builtin() {
        // Every check ID in CHECK_DOCS must have a corresponding entry in BUILTIN_CHECKS
        for doc in super::CHECK_DOCS {
            let def = super::BUILTIN_CHECKS.iter().find(|d| d.id == doc.id);
            assert!(
                def.is_some(),
                "Check '{}' is in CHECK_DOCS but missing from BUILTIN_CHECKS",
                doc.id
            );
        }
    }

    #[test]
    fn contract_no_duplicate_check_ids() {
        // No duplicate check IDs in BUILTIN_CHECKS
        let mut seen = std::collections::HashSet::new();
        for def in super::BUILTIN_CHECKS {
            assert!(
                seen.insert(def.id),
                "Duplicate check ID in BUILTIN_CHECKS: '{}'",
                def.id
            );
        }
    }

    #[test]
    fn contract_no_duplicate_finding_codes() {
        // No duplicate finding codes across all checks (codes should be globally unique)
        let mut seen = std::collections::HashSet::new();
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(
                    seen.insert(*code),
                    "Duplicate finding code '{}' found in check '{}'",
                    code,
                    doc.id
                );
            }
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_check_ids() {
        // explain_check should resolve every check ID
        for def in super::BUILTIN_CHECKS {
            let result = super::explain_check(def.id);
            assert!(
                result.is_some(),
                "explain_check('{}') returned None",
                def.id
            );
        }
    }

    #[test]
    fn contract_explain_check_resolves_all_finding_codes() {
        // explain_check should resolve every finding code to its parent check
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                let result = super::explain_check(code);
                assert!(
                    result.is_some(),
                    "explain_check('{}') returned None for code from check '{}'",
                    code,
                    doc.id
                );
                // Verify it resolves to the correct check
                assert_eq!(
                    result.unwrap().id,
                    doc.id,
                    "explain_check('{}') resolved to '{}', expected '{}'",
                    code,
                    result.unwrap().id,
                    doc.id
                );
            }
        }
    }

    #[test]
    fn contract_all_checks_have_nonempty_codes() {
        // Every documented check should have at least one finding code
        for doc in super::CHECK_DOCS {
            assert!(
                !doc.codes.is_empty(),
                "Check '{}' has no finding codes defined",
                doc.id
            );
        }
    }

    #[test]
    fn contract_check_ids_follow_naming_convention() {
        // Check IDs should follow the pattern "module.check_name"
        for def in super::BUILTIN_CHECKS {
            assert!(
                def.id.contains('.'),
                "Check ID '{}' doesn't follow 'module.check_name' convention",
                def.id
            );
            let parts: Vec<&str> = def.id.split('.').collect();
            assert_eq!(
                parts.len(),
                2,
                "Check ID '{}' should have exactly one '.' separator",
                def.id
            );
            assert!(
                !parts[0].is_empty() && !parts[1].is_empty(),
                "Check ID '{}' has empty module or check name",
                def.id
            );
        }
    }

    #[test]
    fn contract_finding_codes_are_snake_case() {
        // All finding codes should be snake_case (lowercase with underscores, digits allowed)
        for doc in super::CHECK_DOCS {
            for code in doc.codes {
                assert!(
                    code.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "Finding code '{}' in check '{}' is not snake_case",
                    code,
                    doc.id
                );
                // Should not start with a digit
                assert!(
                    !code.chars().next().unwrap_or('0').is_ascii_digit(),
                    "Finding code '{}' in check '{}' starts with a digit",
                    code,
                    doc.id
                );
            }
        }
    }
}
