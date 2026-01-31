use anyhow::{anyhow, Context, Result};
use builddiag_domain::parse_rust_version;
use builddiag_types::Config;
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::{Metadata, MetadataCommand, PackageId};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;

#[derive(Debug, Clone)]
pub struct Toolchain {
    pub path: Utf8PathBuf,
    pub channel: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub is_workspace: bool,
    pub members: Vec<Member>,
    pub workspace_msrv: Option<String>,
    pub workspace_edition: Option<String>,
    pub workspace_resolver: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub name: String,
    pub manifest_path: Utf8PathBuf,
    pub rust_version: Option<String>,
    pub rust_version_workspace: bool,
    pub edition: Option<String>,
    pub edition_workspace: bool,
}

#[derive(Debug, Clone)]
pub struct ToolsChecksums {
    pub path: Utf8PathBuf,
    pub entries: Vec<ChecksumEntry>,
}

#[derive(Debug, Clone)]
pub struct ChecksumEntry {
    pub line: usize,
    pub hash: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolsManifest {
    #[serde(default)]
    pub tool: Vec<ToolDecl>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDecl {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RepoState {
    pub root: Utf8PathBuf,
    pub cargo_root: Option<Utf8PathBuf>,
    pub toolchain: Option<Toolchain>,
    pub workspace: WorkspaceInfo,
    pub tools_checksums: Option<ToolsChecksums>,
    pub tools_manifest: Option<(Utf8PathBuf, ToolsManifest)>,
    pub changed_files: Option<BTreeSet<String>>,
}

pub fn load_repo_state(root: &Utf8Path, config: &Config, changed_files: Option<BTreeSet<String>>) -> Result<RepoState> {
    let root = {
    let pb = fs::canonicalize(root.as_std_path())
        .with_context(|| format!("canonicalize root: {root}"))?;
    Utf8PathBuf::from_path_buf(pb).map_err(|_| anyhow!("non-utf8 repo root path"))?
};

    let cargo_root_candidate = root.join(&config.paths.cargo_root);
    let cargo_root = if cargo_root_candidate.exists() {
        Some(cargo_root_candidate)
    } else {
        None
    };

    let toolchain = find_toolchain(&root, &config.paths.rust_toolchain)?;

    let workspace = if let Some(ref cargo_root) = cargo_root {
        load_workspace(cargo_root)?
    } else {
        WorkspaceInfo {
            is_workspace: false,
            members: Vec::new(),
            workspace_msrv: None,
            workspace_edition: None,
            workspace_resolver: None,
        }
    };

    let tools_checksums = {
        let p = root.join(&config.paths.tools_checksums);
        if p.exists() {
            Some(parse_checksums(&p)?)
        } else {
            None
        }
    };

    let tools_manifest = {
        let p = root.join(&config.paths.tools_manifest);
        if p.exists() {
            let txt = fs::read_to_string(&p)
                .with_context(|| format!("read tools manifest: {p}"))?;
            let manifest: ToolsManifest = toml::from_str(&txt)
                .with_context(|| format!("parse tools manifest: {p}"))?;
            Some((p, manifest))
        } else {
            None
        }
    };

    Ok(RepoState {
        root,
        cargo_root,
        toolchain,
        workspace,
        tools_checksums,
        tools_manifest,
        changed_files,
    })
}

fn find_toolchain(root: &Utf8Path, rust_toolchain_toml_path: &str) -> Result<Option<Toolchain>> {
    let candidate = root.join(rust_toolchain_toml_path);
    if candidate.exists() {
        let channel = parse_rust_toolchain_toml(&candidate)?;
        return Ok(Some(Toolchain {
            path: candidate,
            channel,
        }));
    }

    let legacy = root.join("rust-toolchain");
    if legacy.exists() {
        let channel = fs::read_to_string(&legacy)
            .with_context(|| format!("read {legacy}"))?
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if channel.is_empty() {
            return Err(anyhow!("rust-toolchain is empty"));
        }
        return Ok(Some(Toolchain {
            path: legacy,
            channel,
        }));
    }

    Ok(None)
}

fn parse_rust_toolchain_toml(path: &Utf8Path) -> Result<String> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let v: toml::Value = toml::from_str(&txt).with_context(|| format!("parse {path}"))?;
    // Format: [toolchain] channel = "1.75.0"
    let channel = v
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    if let Some(c) = channel {
        return Ok(c);
    }

    // tolerant fallback: top-level channel
    if let Some(c) = v.get("channel").and_then(|c| c.as_str()) {
        return Ok(c.to_string());
    }

    Err(anyhow!("missing toolchain.channel in {path}"))
}

fn load_workspace(manifest_path: &Utf8Path) -> Result<WorkspaceInfo> {
    let meta = metadata(manifest_path)?;
    let member_ids: BTreeSet<PackageId> = meta.workspace_members.iter().cloned().collect();

    let mut members = Vec::new();
    for pkg in &meta.packages {
        if !member_ids.contains(&pkg.id) {
            continue;
        }
        let manifest_path = Utf8PathBuf::from_path_buf(pkg.manifest_path.clone().into_std_path_buf())
            .map_err(|_| anyhow!("non-utf8 manifest path"))?;

        let manifest_txt = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {manifest_path}"))?;
        let manifest_value: toml::Value = toml::from_str(&manifest_txt)
            .with_context(|| format!("parse {manifest_path}"))?;

        let (rust_version, rust_version_workspace) = parse_package_inheritable_string(&manifest_value, "rust-version")?;
        let (edition, edition_workspace) = parse_package_inheritable_string(&manifest_value, "edition")?;

        members.push(Member {
            name: pkg.name.clone(),
            manifest_path,
            rust_version,
            rust_version_workspace,
            edition,
            edition_workspace,
        });
    }

    // Root manifest info
    let root_txt = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {manifest_path}"))?;
    let root_value: toml::Value = toml::from_str(&root_txt)
        .with_context(|| format!("parse {manifest_path}"))?;

    let (workspace_msrv, workspace_edition, workspace_resolver, is_workspace) = parse_workspace_root(&root_value)?;

    Ok(WorkspaceInfo {
        is_workspace,
        members,
        workspace_msrv,
        workspace_edition,
        workspace_resolver,
    })
}

fn metadata(manifest_path: &Utf8Path) -> Result<Metadata> {
    let mut cmd = MetadataCommand::new();
    cmd.manifest_path(manifest_path.as_str());
    cmd.no_deps();
    cmd.exec().with_context(|| format!("cargo metadata for {manifest_path}"))
}

fn parse_workspace_root(v: &toml::Value) -> Result<(Option<String>, Option<String>, Option<String>, bool)> {
    // workspace.package.rust-version
    let workspace_msrv = v
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(|rv| rv.as_str())
        .map(|s| s.to_string());

    let workspace_edition = v
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("edition"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    let workspace_resolver = v
        .get("workspace")
        .and_then(|w| w.get("resolver"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    // If [workspace] exists, treat as workspace.
    let is_workspace = v.get("workspace").is_some();

    // Non-workspace package may still have package.rust-version; treat as "workspace" for our purposes.
    let package_msrv = v
        .get("package")
        .and_then(|p| p.get("rust-version"))
        .and_then(|rv| rv.as_str())
        .map(|s| s.to_string());

    let msrv = workspace_msrv.or(package_msrv);

    Ok((msrv, workspace_edition, workspace_resolver, is_workspace))
}

fn parse_package_inheritable_string(v: &toml::Value, key: &str) -> Result<(Option<String>, bool)> {
    let pkg = match v.get("package") {
        Some(p) => p,
        None => return Ok((None, false)),
    };

    match pkg.get(key) {
        None => Ok((None, false)),
        Some(toml::Value::String(s)) => Ok((Some(s.clone()), false)),
        Some(toml::Value::Table(tbl)) => {
            // Inheritable field syntax: key.workspace = true
            let workspace = tbl
                .get("workspace")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok((None, workspace))
        }
        Some(_) => Err(anyhow!("unsupported type for package.{key}")),
    }
}

fn parse_checksums(path: &Utf8Path) -> Result<ToolsChecksums> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut entries = Vec::new();

    for (idx, line) in txt.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // common format: <hash><space(s)><path>
        let mut parts = trimmed.split_whitespace();
        let hash = match parts.next() {
            Some(h) => h.to_string(),
            None => continue,
        };
        let path_part = match parts.next() {
            Some(p) => p.to_string(),
            None => "".to_string(),
        };
        entries.push(ChecksumEntry {
            line: line_no,
            hash,
            path: path_part,
        });
    }

    Ok(ToolsChecksums {
        path: path.to_path_buf(),
        entries,
    })
}

/// A helper for checks that need normalized versions.
///
/// Returns `Ok(None)` if input is non-numeric (stable/beta/nightly).
pub fn maybe_parse_numeric_version(s: &str) -> Result<Option<String>> {
    let t = s.trim();
    let base = t.split_once("-").map(|(a, _)| a).unwrap_or(t);
    if base.eq_ignore_ascii_case("stable") || base.eq_ignore_ascii_case("beta") || base.eq_ignore_ascii_case("nightly") {
        return Ok(None);
    }
    // Accept and normalize; this will error for invalid.
    let v = parse_rust_version(base)?;
    Ok(Some(v.to_string()))
}
