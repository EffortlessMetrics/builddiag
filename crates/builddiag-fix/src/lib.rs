//! Auto-fix planner and applier for builddiag.
//!
//! This crate implements deterministic, unambiguous fixes for a subset of
//! build-contract findings.

use anyhow::{Context, Result, anyhow};
use builddiag_domain::parse_rust_version;
use builddiag_repo::{
    RepoState, load_repo_state, maybe_parse_numeric_version, parse_checksums_content,
};
use builddiag_types::Config;
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Kinds of auto-fixes supported by this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FixKind {
    /// Add `workspace.package.rust-version` in root `Cargo.toml`.
    WorkspaceMsrv,
    /// Set `workspace.resolver = "2"` in root `Cargo.toml`.
    WorkspaceResolver,
    /// Add missing entries to checksums file from tools manifest.
    ChecksumsEntries,
}

impl FixKind {
    /// Returns a stable identifier for the fix kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceMsrv => "workspace_msrv",
            Self::WorkspaceResolver => "workspace_resolver_v2",
            Self::ChecksumsEntries => "checksums_entries",
        }
    }
}

/// A planned fix action suitable for display and confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixProposal {
    /// Fix category.
    pub kind: FixKind,
    /// Target file path.
    pub target: Utf8PathBuf,
    /// Human-readable summary of the change.
    pub summary: String,
}

/// Planned fixes and non-fatal planning warnings.
#[derive(Debug, Clone, Default)]
pub struct FixPlan {
    /// Proposed fix actions.
    pub proposals: Vec<FixProposal>,
    /// Planning warnings where auto-fix was skipped as ambiguous/unsafe.
    pub warnings: Vec<String>,
}

/// Apply-mode options for fix execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptions {
    /// Do not write files; report what would be changed.
    pub dry_run: bool,
    /// Require per-fix confirmation through the provided callback.
    pub interactive: bool,
}

/// Result of applying planned fixes.
#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    /// Number of planned proposals.
    pub planned: usize,
    /// Number of proposals written to disk.
    pub applied: usize,
    /// Number of proposals accepted in dry-run mode.
    pub dry_run_actions: usize,
    /// Number of proposals declined in interactive mode.
    pub skipped: usize,
    /// Non-fatal warnings produced while planning.
    pub warnings: Vec<String>,
    /// Files modified on disk.
    pub changed_files: BTreeSet<Utf8PathBuf>,
}

#[derive(Debug, Clone)]
enum Action {
    SetWorkspaceMsrv {
        manifest_path: Utf8PathBuf,
        version: String,
    },
    SetWorkspaceResolverV2 {
        manifest_path: Utf8PathBuf,
    },
    AddChecksumsEntries {
        checksums_path: Utf8PathBuf,
        entries: Vec<(String, String)>,
    },
}

impl Action {
    fn proposal(&self) -> FixProposal {
        match self {
            Self::SetWorkspaceMsrv {
                manifest_path,
                version,
            } => FixProposal {
                kind: FixKind::WorkspaceMsrv,
                target: manifest_path.clone(),
                summary: format!("set workspace.package.rust-version = \"{version}\""),
            },
            Self::SetWorkspaceResolverV2 { manifest_path } => FixProposal {
                kind: FixKind::WorkspaceResolver,
                target: manifest_path.clone(),
                summary: "set workspace.resolver = \"2\"".to_string(),
            },
            Self::AddChecksumsEntries {
                checksums_path,
                entries,
            } => FixProposal {
                kind: FixKind::ChecksumsEntries,
                target: checksums_path.clone(),
                summary: format!(
                    "add {} missing checksum entr{}",
                    entries.len(),
                    if entries.len() == 1 { "y" } else { "ies" }
                ),
            },
        }
    }
}

/// Builds a deterministic fix plan for the given repository/config.
pub fn plan_fixes(root: &Utf8Path, config: &Config) -> Result<FixPlan> {
    let (actions, warnings) = compute_actions(root, config)?;
    let proposals = actions.iter().map(Action::proposal).collect();
    Ok(FixPlan {
        proposals,
        warnings,
    })
}

/// Applies deterministic fixes using optional interactive confirmation.
///
/// The `confirm` callback is only invoked when `options.interactive` is true.
pub fn apply_fixes<F>(
    root: &Utf8Path,
    config: &Config,
    options: ApplyOptions,
    mut confirm: F,
) -> Result<ApplyResult>
where
    F: FnMut(&FixProposal) -> Result<bool>,
{
    let (actions, warnings) = compute_actions(root, config)?;
    let proposals: Vec<FixProposal> = actions.iter().map(Action::proposal).collect();

    let mut result = ApplyResult {
        planned: proposals.len(),
        warnings,
        ..ApplyResult::default()
    };

    for (action, proposal) in actions.into_iter().zip(proposals) {
        if options.interactive && !confirm(&proposal)? {
            result.skipped += 1;
            continue;
        }

        if options.dry_run {
            result.dry_run_actions += 1;
            continue;
        }

        let changed = match action {
            Action::SetWorkspaceMsrv {
                manifest_path,
                version,
            } => apply_workspace_manifest_changes(&manifest_path, Some(&version), false)?,
            Action::SetWorkspaceResolverV2 { manifest_path } => {
                apply_workspace_manifest_changes(&manifest_path, None, true)?
            }
            Action::AddChecksumsEntries {
                checksums_path,
                entries,
            } => apply_checksums_entries(&checksums_path, &entries)?,
        };

        if changed {
            result.applied += 1;
            result.changed_files.insert(proposal.target);
        }
    }

    Ok(result)
}

fn compute_actions(root: &Utf8Path, config: &Config) -> Result<(Vec<Action>, Vec<String>)> {
    let repo = load_repo_state(root, config, None)?;
    let mut actions = Vec::new();
    let mut warnings = Vec::new();

    plan_workspace_msrv_fix(&repo, &mut actions, &mut warnings)?;
    plan_workspace_resolver_fix(&repo, &mut actions);
    plan_checksums_fix(&repo, config, &mut actions, &mut warnings)?;

    Ok((actions, warnings))
}

fn plan_workspace_msrv_fix(
    repo: &RepoState,
    actions: &mut Vec<Action>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    if !repo.workspace.is_workspace || repo.workspace.workspace_msrv.is_some() {
        return Ok(());
    }
    let Some(cargo_root) = repo.cargo_root.clone() else {
        return Ok(());
    };

    if let Some(version) = derive_workspace_msrv(repo, warnings)? {
        actions.push(Action::SetWorkspaceMsrv {
            manifest_path: cargo_root,
            version,
        });
    }

    Ok(())
}

fn derive_workspace_msrv(repo: &RepoState, warnings: &mut Vec<String>) -> Result<Option<String>> {
    if let Some(toolchain) = &repo.toolchain
        && let Some(version) = maybe_parse_numeric_version(&toolchain.channel)?
    {
        return Ok(Some(version));
    }

    let mut member_versions = BTreeSet::new();
    for member in &repo.workspace.members {
        let Some(msrv_raw) = &member.rust_version else {
            continue;
        };
        match parse_rust_version(msrv_raw) {
            Ok(parsed) => {
                member_versions.insert(parsed.to_string());
            }
            Err(err) => {
                warnings.push(format!(
                    "Skipping invalid rust-version '{}' in {}: {err}",
                    msrv_raw, member.manifest_path
                ));
            }
        }
    }

    if member_versions.len() == 1 {
        return Ok(member_versions.into_iter().next());
    }

    if member_versions.is_empty() {
        warnings.push(
            "Cannot auto-add workspace rust-version: no numeric toolchain or member rust-version found"
                .to_string(),
        );
    } else {
        let versions = member_versions.into_iter().collect::<Vec<_>>().join(", ");
        warnings.push(format!(
            "Cannot auto-add workspace rust-version: members declare multiple versions ({versions})"
        ));
    }
    Ok(None)
}

fn plan_workspace_resolver_fix(repo: &RepoState, actions: &mut Vec<Action>) {
    if !repo.workspace.is_workspace {
        return;
    }
    if repo.workspace.workspace_resolver.as_deref() == Some("2") {
        return;
    }
    if let Some(cargo_root) = repo.cargo_root.clone() {
        actions.push(Action::SetWorkspaceResolverV2 {
            manifest_path: cargo_root,
        });
    }
}

fn plan_checksums_fix(
    repo: &RepoState,
    config: &Config,
    actions: &mut Vec<Action>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let Some((_manifest_path, manifest)) = &repo.tools_manifest else {
        return Ok(());
    };

    let mut expected = BTreeSet::new();
    for tool in &manifest.tool {
        for file in &tool.files {
            let trimmed = file.trim();
            if !trimmed.is_empty() {
                expected.insert(trimmed.to_string());
            }
        }
    }
    if expected.is_empty() {
        return Ok(());
    }

    let have: BTreeSet<String> = repo
        .tools_checksums
        .as_ref()
        .map(|cks| cks.entries.iter().map(|entry| entry.path.clone()).collect())
        .unwrap_or_default();

    let mut entries = Vec::new();
    for rel in expected.difference(&have) {
        let abs = repo.root.join(rel);
        if !abs.exists() || !abs.is_file() {
            warnings.push(format!(
                "Cannot generate checksum for missing tool file '{}'",
                rel
            ));
            continue;
        }
        let bytes = fs::read(&abs).with_context(|| format!("read tool file {abs}"))?;
        entries.push((rel.clone(), sha256_hex(&bytes)));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if entries.is_empty() {
        return Ok(());
    }

    actions.push(Action::AddChecksumsEntries {
        checksums_path: repo.root.join(&config.paths.tools_checksums),
        entries,
    });

    Ok(())
}

fn apply_workspace_manifest_changes(
    manifest_path: &Utf8Path,
    workspace_msrv: Option<&str>,
    set_resolver_v2: bool,
) -> Result<bool> {
    let raw = fs::read_to_string(manifest_path).with_context(|| format!("read {manifest_path}"))?;
    let mut value: toml::Value =
        toml::from_str(&raw).with_context(|| format!("parse {manifest_path}"))?;

    let root = value
        .as_table_mut()
        .ok_or_else(|| anyhow!("manifest root is not a table: {manifest_path}"))?;
    let workspace = ensure_table(root, "workspace")?;

    let mut changed = false;

    if set_resolver_v2 && workspace.get("resolver").and_then(toml::Value::as_str) != Some("2") {
        workspace.insert("resolver".to_string(), toml::Value::String("2".to_string()));
        changed = true;
    }

    if let Some(msrv) = workspace_msrv {
        let package = ensure_table(workspace, "package")?;
        if package.get("rust-version").and_then(toml::Value::as_str) != Some(msrv) {
            package.insert(
                "rust-version".to_string(),
                toml::Value::String(msrv.to_string()),
            );
            changed = true;
        }
    }

    if !changed {
        return Ok(false);
    }

    let rendered = toml::to_string_pretty(&value)
        .with_context(|| format!("render updated manifest {manifest_path}"))?;
    fs::write(manifest_path, rendered).with_context(|| format!("write {manifest_path}"))?;

    Ok(true)
}

fn ensure_table<'a>(parent: &'a mut toml::Table, key: &str) -> Result<&'a mut toml::Table> {
    let needs_insert = !matches!(parent.get(key), Some(toml::Value::Table(_)));
    if needs_insert {
        parent.insert(key.to_string(), toml::Value::Table(toml::Table::new()));
    }
    parent
        .get_mut(key)
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| anyhow!("expected table at key '{key}'"))
}

fn apply_checksums_entries(path: &Utf8Path, entries: &[(String, String)]) -> Result<bool> {
    let mut existing = BTreeMap::<String, String>::new();
    if path.exists() {
        let raw = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        for entry in parse_checksums_content(&raw) {
            if entry.path.is_empty() {
                continue;
            }
            existing.insert(entry.path, entry.hash);
        }
    }

    let mut changed = false;
    for (rel, hash) in entries {
        if existing.get(rel) != Some(hash) {
            existing.insert(rel.clone(), hash.clone());
            changed = true;
        }
    }

    if !changed {
        return Ok(false);
    }

    let mut out = String::new();
    for (rel, hash) in existing {
        out.push_str(&format!("{hash}  {rel}\n"));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {parent}"))?;
    }
    fs::write(path, out).with_context(|| format!("write {path}"))?;
    Ok(true)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(root: &Utf8Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn create_workspace() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        write_file(
            &root,
            "Cargo.toml",
            r#"[workspace]
members = ["crates/a"]
"#,
        );
        write_file(
            &root,
            "crates/a/Cargo.toml",
            r#"[package]
    name = "a"
    version = "0.1.0"
    edition = "2021"
    rust-version = "1.92"
    "#,
        );
        write_file(&root, "crates/a/src/lib.rs", "pub fn a() {}\n");
        write_file(
            &root,
            "rust-toolchain.toml",
            r#"[toolchain]
    channel = "1.92.0"
    "#,
        );
        write_file(
            &root,
            "scripts/tools.toml",
            r#"
[[tool]]
name = "demo"
files = ["scripts/tool.sh"]
"#,
        );
        write_file(&root, "scripts/tool.sh", "echo demo\n");

        (temp, root)
    }

    #[test]
    fn plan_includes_msrv_resolver_and_checksums() {
        let (_temp, root) = create_workspace();
        let cfg = Config::default();

        let plan = plan_fixes(&root, &cfg).unwrap();
        let kinds: BTreeSet<FixKind> = plan.proposals.iter().map(|p| p.kind).collect();

        assert!(kinds.contains(&FixKind::WorkspaceMsrv));
        assert!(kinds.contains(&FixKind::WorkspaceResolver));
        assert!(kinds.contains(&FixKind::ChecksumsEntries));
    }

    #[test]
    fn apply_writes_manifest_and_checksums() {
        let (_temp, root) = create_workspace();
        let cfg = Config::default();

        let result = apply_fixes(&root, &cfg, ApplyOptions::default(), |_| Ok(true)).unwrap();
        assert_eq!(result.planned, 3);
        assert_eq!(result.applied, 3);

        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("resolver = \"2\""));
        assert!(manifest.contains("rust-version = \"1.92.0\""));

        let checksums = std::fs::read_to_string(root.join("scripts/tools.sha256")).unwrap();
        assert!(checksums.contains("scripts/tool.sh"));
    }

    #[test]
    fn dry_run_does_not_write_files() {
        let (_temp, root) = create_workspace();
        let cfg = Config::default();
        let before_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();

        let result = apply_fixes(
            &root,
            &cfg,
            ApplyOptions {
                dry_run: true,
                interactive: false,
            },
            |_| Ok(true),
        )
        .unwrap();

        assert_eq!(result.applied, 0);
        assert_eq!(result.dry_run_actions, result.planned);
        let after_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert_eq!(before_manifest, after_manifest);
        assert!(!root.join("scripts/tools.sha256").exists());
    }

    #[test]
    fn ambiguous_member_msrv_emits_warning_and_skips_msrv_fix() {
        let (_temp, root) = create_workspace();
        write_file(
            &root,
            "Cargo.toml",
            r#"[workspace]
members = ["crates/a", "crates/b"]
"#,
        );
        write_file(
            &root,
            "crates/b/Cargo.toml",
            r#"[package]
name = "b"
version = "0.1.0"
edition = "2021"
rust-version = "1.74"
"#,
        );
        write_file(&root, "crates/b/src/lib.rs", "pub fn b() {}\n");
        std::fs::remove_file(root.join("rust-toolchain.toml")).unwrap();

        let cfg = Config::default();
        let plan = plan_fixes(&root, &cfg).unwrap();
        let kinds: BTreeSet<FixKind> = plan.proposals.iter().map(|p| p.kind).collect();
        assert!(!kinds.contains(&FixKind::WorkspaceMsrv));
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("multiple versions"))
        );
    }
}
