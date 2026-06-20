//! Repository state loading for builddiag.
//!
//! This crate provides functionality for discovering and loading workspace
//! information from Cargo repositories. It supports:
//!
//! - Multi-crate workspaces with glob patterns in members/exclude
//! - Single-crate repositories (treated as "workspace of one")
//! - Virtual workspaces (workspace with no root package)
//! - Deterministic ordering of discovered members
//! - Incremental caching of parsed state (with `cache` feature)
//!
//! # Path Normalization
//!
//! All paths are normalized to use forward slashes regardless of platform,
//! and are expressed as repo-relative paths where appropriate.
//!
//! # Caching
//!
//! When the `cache` feature is enabled (default), repository state can be
//! cached to disk for faster subsequent loads. The cache uses file modification
//! times and content hashes to detect changes.

#[cfg(feature = "cache")]
pub mod cache;

use anyhow::{Context, Result, anyhow};
use builddiag_domain::parse_rust_version;
pub use builddiag_paths::{join_normalized, normalize_slashes, to_repo_relative};
use builddiag_types::Config;
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::{Metadata, MetadataCommand, PackageId};
use globset::{Glob, GlobSetBuilder};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[cfg(feature = "cache")]
pub use cache::{CacheConfig, RepoStateCache};

// ============================================================================
// Workspace Model Types
// ============================================================================

/// Parsed representation of a Cargo.toml manifest.
#[derive(Debug, Clone)]
pub struct ParsedManifest {
    /// The raw TOML value of the manifest.
    pub value: toml::Value,
    /// The package name, if this is a package manifest.
    pub package_name: Option<String>,
    /// The rust-version field, if present (not inherited).
    pub rust_version: Option<String>,
    /// Whether rust-version inherits from workspace.
    pub rust_version_workspace: bool,
    /// The edition field, if present (not inherited).
    pub edition: Option<String>,
    /// Whether edition inherits from workspace.
    pub edition_workspace: bool,
}

/// Comprehensive model of a Cargo workspace.
///
/// This type represents the complete parsed state of a workspace, including
/// the root manifest and all member manifests. It supports both multi-crate
/// workspaces and single-crate repositories (treated as "workspace of one").
#[derive(Debug, Clone)]
pub struct WorkspaceModel {
    /// Parsed root Cargo.toml manifest.
    pub root_manifest: ParsedManifest,
    /// Map of repo-relative paths to parsed member manifests.
    /// Uses BTreeMap for deterministic ordering.
    pub member_manifests: BTreeMap<String, ParsedManifest>,
    /// Whether this is a virtual workspace (has [workspace] but no [package]).
    pub is_virtual: bool,
    /// The workspace MSRV, if defined at workspace level.
    pub workspace_msrv: Option<String>,
    /// The workspace edition, if defined at workspace level.
    pub workspace_edition: Option<String>,
    /// The workspace resolver version, if defined.
    pub workspace_resolver: Option<String>,
    /// The list of member patterns from [workspace.members].
    pub member_patterns: Vec<String>,
    /// The list of exclude patterns from [workspace.exclude].
    pub exclude_patterns: Vec<String>,
}

impl WorkspaceModel {
    /// Returns whether this workspace has any members.
    pub fn has_members(&self) -> bool {
        !self.member_manifests.is_empty()
    }

    /// Returns the number of workspace members.
    pub fn member_count(&self) -> usize {
        self.member_manifests.len()
    }

    /// Returns all member paths in sorted order.
    pub fn member_paths(&self) -> Vec<&str> {
        self.member_manifests.keys().map(|s| s.as_str()).collect()
    }
}

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
    /// Whether this member has at least one binary target (explicit [[bin]] or src/main.rs).
    pub has_binary_target: bool,
    /// Package metadata for publish readiness checks.
    pub publish_metadata: PublishMetadata,
}

/// Metadata relevant for publishing to crates.io.
#[derive(Debug, Clone, Default)]
pub struct PublishMetadata {
    /// Whether package.publish is explicitly set to false.
    pub publish_disabled: bool,
    /// Package description field.
    pub description: Option<String>,
    /// Package license field.
    pub license: Option<String>,
    /// Package license-file field.
    pub license_file: Option<String>,
    /// Package repository field.
    pub repository: Option<String>,
    /// Package homepage field.
    pub homepage: Option<String>,
    /// Package documentation field.
    pub documentation: Option<String>,
    /// Package readme field.
    pub readme: Option<String>,
    /// Package keywords field (up to 5).
    pub keywords: Vec<String>,
    /// Package categories field (up to 5).
    pub categories: Vec<String>,
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
    /// The comprehensive workspace model with all discovery information.
    /// This provides direct access to parsed manifests and workspace patterns.
    pub workspace_model: Option<WorkspaceModel>,
    pub tools_checksums: Option<ToolsChecksums>,
    pub tools_manifest: Option<(Utf8PathBuf, ToolsManifest)>,
    pub changed_files: Option<BTreeSet<String>>,
    /// Whether Cargo.lock exists at the workspace root.
    pub lockfile_exists: bool,
}

pub fn load_repo_state(
    root: &Utf8Path,
    config: &Config,
    changed_files: Option<BTreeSet<String>>,
) -> Result<RepoState> {
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

    let (workspace, workspace_model) = if let Some(ref cargo_root) = cargo_root {
        let ws = load_workspace(cargo_root)?;
        // Note: workspace_model is now computed lazily by the member_ordering check
        // when needed, avoiding the overhead of discover_workspace for most runs.
        // Only load it here if explicitly needed for caching.
        #[cfg(feature = "cache")]
        let model = discover_workspace(cargo_root).ok();
        #[cfg(not(feature = "cache"))]
        let model = None;
        (ws, model)
    } else {
        (
            WorkspaceInfo {
                is_workspace: false,
                members: Vec::new(),
                workspace_msrv: None,
                workspace_edition: None,
                workspace_resolver: None,
            },
            None,
        )
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
            let txt =
                fs::read_to_string(&p).with_context(|| format!("read tools manifest: {p}"))?;
            let manifest: ToolsManifest =
                toml::from_str(&txt).with_context(|| format!("parse tools manifest: {p}"))?;
            Some((p, manifest))
        } else {
            None
        }
    };

    // Check for Cargo.lock at the workspace root
    let lockfile_exists = if let Some(ref cargo_root) = cargo_root {
        cargo_root
            .parent()
            .is_some_and(|p| p.join("Cargo.lock").exists())
    } else {
        root.join("Cargo.lock").exists()
    };

    Ok(RepoState {
        root,
        cargo_root,
        toolchain,
        workspace,
        workspace_model,
        tools_checksums,
        tools_manifest,
        changed_files,
        lockfile_exists,
    })
}

/// Load repository state with caching support.
///
/// This function extends [`load_repo_state`] with optional caching to improve
/// performance on subsequent runs. When caching is enabled:
///
/// 1. The cache is loaded from disk
/// 2. File modification times and content hashes are checked
/// 3. Only changed files are re-parsed
/// 4. The updated cache is written back to disk
///
/// # Arguments
///
/// * `root` - Repository root path
/// * `config` - Build configuration
/// * `changed_files` - Optional set of changed files for diff-aware mode
/// * `cache_config` - Cache configuration (set to `None` to disable caching)
///
/// # Example
///
/// ```ignore
/// use builddiag_repo::{load_repo_state_cached, CacheConfig};
///
/// let cache_config = CacheConfig::default();
/// let state = load_repo_state_cached(root, &config, None, Some(&cache_config))?;
/// ```
#[cfg(feature = "cache")]
pub fn load_repo_state_cached(
    root: &Utf8Path,
    config: &Config,
    changed_files: Option<BTreeSet<String>>,
    cache_config: Option<&CacheConfig>,
) -> Result<RepoState> {
    // If caching is disabled, fall back to regular load
    let Some(cache_cfg) = cache_config else {
        return load_repo_state(root, config, changed_files);
    };

    if !cache_cfg.enabled {
        return load_repo_state(root, config, changed_files);
    }

    let root = {
        let pb = fs::canonicalize(root.as_std_path())
            .with_context(|| format!("canonicalize root: {root}"))?;
        Utf8PathBuf::from_path_buf(pb).map_err(|_| anyhow!("non-utf8 repo root path"))?
    };

    let cache_dir = cache_cfg.cache_dir_abs(&root);

    // Try to load existing cache
    let existing_cache = cache::RepoStateCache::load(&cache_dir).ok().flatten();

    // Load state (potentially using cached data in the future)
    // For now, we do a full load but save the cache for next time
    let state = load_repo_state(&root, config, changed_files)?;

    // Build and save cache for next time
    let new_cache = build_cache_from_state(&root, &state, config)?;
    if let Err(e) = new_cache.save(&cache_dir) {
        // Log warning but don't fail the operation
        eprintln!("builddiag: warning: failed to save cache: {e}");
    }

    // For future optimization: compare existing_cache with file mtimes and
    // selectively reload only changed files. For now, we always do a full load
    // but maintain the cache infrastructure.
    let _ = existing_cache; // Suppress unused warning for now

    Ok(state)
}

/// Build a cache structure from the current repo state.
#[cfg(feature = "cache")]
fn build_cache_from_state(
    root: &Utf8Path,
    state: &RepoState,
    _config: &Config,
) -> Result<cache::RepoStateCache> {
    use cache::*;

    let mut cache = RepoStateCache::new();

    // Cache Cargo.toml
    state
        .cargo_root
        .as_ref()
        .map(|cargo_root| {
            cache.cargo_root.meta = get_file_meta(root, cargo_root)?;
            cache.cargo_root.workspace_msrv = state.workspace.workspace_msrv.clone();
            cache.cargo_root.workspace_edition = state.workspace.workspace_edition.clone();
            cache.cargo_root.workspace_resolver = state.workspace.workspace_resolver.clone();
            cache.cargo_root.is_workspace = state.workspace.is_workspace;

            if let Some(ref model) = state.workspace_model {
                cache.cargo_root.member_patterns = model.member_patterns.clone();
                cache.cargo_root.exclude_patterns = model.exclude_patterns.clone();
            }

            Ok::<(), anyhow::Error>(())
        })
        .transpose()?;

    // Cache rust-toolchain.toml
    if let Some(ref tc) = state.toolchain {
        cache.toolchain.meta = get_file_meta(root, &tc.path)?;
        cache.toolchain.channel = Some(tc.channel.clone());
    }

    // Cache checksums
    if let Some(ref cks) = state.tools_checksums {
        cache.checksums.meta = get_file_meta(root, &cks.path)?;
        cache.checksums.entry_count = cks.entries.len();
    }

    // Cache workspace members
    for member in &state.workspace.members {
        let rel_path = to_repo_relative(root, &member.manifest_path);
        if let Some(meta) = get_file_meta(root, &member.manifest_path)? {
            cache.members.insert(
                rel_path,
                CachedMember {
                    meta,
                    name: member.name.clone(),
                    rust_version: member.rust_version.clone(),
                    rust_version_workspace: member.rust_version_workspace,
                    edition: member.edition.clone(),
                    edition_workspace: member.edition_workspace,
                    has_binary_target: member.has_binary_target,
                },
            );
        }
    }

    cache.lockfile_exists = state.lockfile_exists;

    Ok(cache)
}

/// Build a [`RepoState`] from a pre-computed [`Substrate`], skipping all
/// filesystem operations.
///
/// This is the substrate bridge entry point: when an upstream tool has
/// already parsed the workspace, it can pass a [`Substrate`] to avoid
/// redundant disk I/O.
///
/// # Arguments
///
/// * `root` - Repository root path (used for path context, no I/O performed)
/// * `substrate` - Pre-computed repository state from upstream
pub fn repo_state_from_substrate(
    root: &Utf8Path,
    substrate: &builddiag_types::Substrate,
) -> RepoState {
    let toolchain = if substrate.has_toolchain {
        substrate.toolchain_channel.as_ref().map(|ch| Toolchain {
            path: root.join("rust-toolchain.toml"),
            channel: ch.clone(),
        })
    } else {
        None
    };

    let members: Vec<Member> = substrate
        .manifests
        .iter()
        .map(|m| Member {
            name: m.name.clone().unwrap_or_default(),
            manifest_path: root.join(&m.path),
            rust_version: m.msrv.clone(),
            rust_version_workspace: false,
            edition: m.edition.clone(),
            edition_workspace: false,
            has_binary_target: false,
            publish_metadata: PublishMetadata::default(),
        })
        .collect();

    let workspace = WorkspaceInfo {
        is_workspace: substrate.manifests.len() > 1,
        members,
        workspace_msrv: substrate.workspace_msrv.clone(),
        workspace_edition: None,
        workspace_resolver: None,
    };

    RepoState {
        root: root.to_path_buf(),
        cargo_root: Some(root.join("Cargo.toml")),
        toolchain,
        workspace,
        workspace_model: None,
        tools_checksums: None,
        tools_manifest: None,
        changed_files: None,
        lockfile_exists: substrate.has_lockfile,
    }
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
    for pkg in meta
        .packages
        .iter()
        .filter(|pkg| member_ids.contains(&pkg.id))
    {
        let manifest_path =
            Utf8PathBuf::from_path_buf(pkg.manifest_path.clone().into_std_path_buf())
                .map_err(|_| anyhow!("non-utf8 manifest path"))?;

        let manifest_txt =
            fs::read_to_string(&manifest_path).with_context(|| format!("read {manifest_path}"))?;
        let manifest_value: toml::Value =
            toml::from_str(&manifest_txt).with_context(|| format!("parse {manifest_path}"))?;

        let (rust_version, rust_version_workspace) =
            parse_package_inheritable_string(&manifest_value, "rust-version")?;
        let (edition, edition_workspace) =
            parse_package_inheritable_string(&manifest_value, "edition")?;

        // Check for binary targets
        let has_binary = has_binary_target(&manifest_path, &manifest_value)?;

        // Parse publish metadata
        let publish_metadata = parse_publish_metadata(&manifest_value);

        members.push(Member {
            name: pkg.name.to_string(),
            manifest_path,
            rust_version,
            rust_version_workspace,
            edition,
            edition_workspace,
            has_binary_target: has_binary,
            publish_metadata,
        });
    }

    // Root manifest info
    let root_txt =
        fs::read_to_string(manifest_path).with_context(|| format!("read {manifest_path}"))?;
    let root_value: toml::Value =
        toml::from_str(&root_txt).with_context(|| format!("parse {manifest_path}"))?;

    let (workspace_msrv, workspace_edition, workspace_resolver, is_workspace) =
        parse_workspace_root(&root_value)?;

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
    cmd.exec()
        .with_context(|| format!("cargo metadata for {manifest_path}"))
}

fn parse_workspace_root(
    v: &toml::Value,
) -> Result<(Option<String>, Option<String>, Option<String>, bool)> {
    #![allow(clippy::type_complexity)]
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

/// Determines whether a Cargo manifest has binary targets.
///
/// A package has binary targets if:
/// - It has an explicit `[[bin]]` section with at least one entry, OR
/// - It has a `src/main.rs` file in the package directory
fn has_binary_target(manifest_path: &Utf8Path, manifest: &toml::Value) -> Result<bool> {
    // Check for explicit [[bin]] section
    if let Some(bins) = manifest.get("bin").and_then(|b| b.as_array())
        && !bins.is_empty()
    {
        return Ok(true);
    }

    // Check for src/main.rs
    let src_main = manifest_path.parent().unwrap().join("src/main.rs");
    if src_main.exists() {
        return Ok(true);
    }

    Ok(false)
}

/// Parses publish metadata from a Cargo.toml manifest.
fn parse_publish_metadata(manifest: &toml::Value) -> PublishMetadata {
    let pkg = match manifest.get("package") {
        Some(p) => p,
        None => return PublishMetadata::default(),
    };

    // Check if publish is disabled
    let publish_disabled = match pkg.get("publish") {
        Some(toml::Value::Boolean(false)) => true,
        Some(toml::Value::Array(arr)) if arr.is_empty() => true,
        _ => false,
    };

    let description = pkg
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let license = pkg
        .get("license")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let license_file = pkg
        .get("license-file")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let repository = pkg
        .get("repository")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let homepage = pkg
        .get("homepage")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let documentation = pkg
        .get("documentation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let readme = pkg
        .get("readme")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let keywords = pkg
        .get("keywords")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let categories = pkg
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    PublishMetadata {
        publish_disabled,
        description,
        license,
        license_file,
        repository,
        homepage,
        documentation,
        readme,
        keywords,
        categories,
    }
}

// ============================================================================
// Workspace Discovery
// ============================================================================

/// Discovers workspace members by parsing Cargo.toml and expanding glob patterns.
///
/// This function implements the full workspace discovery algorithm:
/// 1. Parses the root Cargo.toml to find [workspace.members] and [workspace.exclude]
/// 2. Expands glob patterns in both arrays
/// 3. Filters out excluded paths
/// 4. Returns all member manifest paths in sorted order
///
/// For single-crate repos (no [workspace] section), it treats the repo as a
/// "workspace of one" with the root manifest as the only member.
///
/// # Arguments
///
/// * `root_manifest_path` - Path to the root Cargo.toml file
///
/// # Returns
///
/// A `WorkspaceModel` containing all discovered workspace information.
pub fn discover_workspace(root_manifest_path: &Utf8Path) -> Result<WorkspaceModel> {
    // Get the workspace root directory
    let workspace_root = root_manifest_path
        .parent()
        .ok_or_else(|| anyhow!("manifest path has no parent directory: {root_manifest_path}"))?;

    // Parse the root manifest
    let root_txt = fs::read_to_string(root_manifest_path)
        .with_context(|| format!("read {root_manifest_path}"))?;
    let root_value: toml::Value =
        toml::from_str(&root_txt).with_context(|| format!("parse {root_manifest_path}"))?;

    // Check if this is a workspace
    let has_workspace_section = root_value.get("workspace").is_some();
    let has_package_section = root_value.get("package").is_some();

    // Parse root manifest fields
    let root_manifest = parse_manifest(&root_value)?;

    // Extract workspace-level settings
    let (workspace_msrv, workspace_edition, workspace_resolver) =
        extract_workspace_settings(&root_value)?;

    // Determine if this is a virtual workspace
    let is_virtual = has_workspace_section && !has_package_section;

    if !has_workspace_section {
        // Single-crate repo: treat as "workspace of one"
        let relative_path = "Cargo.toml".to_string();
        let mut member_manifests = BTreeMap::new();
        member_manifests.insert(relative_path, root_manifest.clone());

        return Ok(WorkspaceModel {
            root_manifest,
            member_manifests,
            is_virtual: false,
            workspace_msrv,
            workspace_edition,
            workspace_resolver,
            member_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        });
    }

    // Parse members and exclude patterns
    let member_patterns = extract_string_array(&root_value, &["workspace", "members"]);
    let exclude_patterns = extract_string_array(&root_value, &["workspace", "exclude"]);

    // Discover all member paths
    let member_paths =
        expand_workspace_patterns(workspace_root, &member_patterns, &exclude_patterns)?;

    // Parse each member manifest
    let mut member_manifests = BTreeMap::new();
    for member_path in member_paths {
        let manifest_path = join_normalized(workspace_root, &member_path).join("Cargo.toml");
        let txt = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read member manifest: {manifest_path}"))?;
        let value: toml::Value = toml::from_str(&txt)
            .with_context(|| format!("parse member manifest: {manifest_path}"))?;
        let manifest = parse_manifest(&value)?;
        let relative_manifest_path = format!("{}/Cargo.toml", member_path);
        member_manifests.insert(relative_manifest_path, manifest);
    }

    // For non-virtual workspaces, also include the root package as a member
    if !is_virtual && has_package_section {
        member_manifests.insert("Cargo.toml".to_string(), root_manifest.clone());
    }

    Ok(WorkspaceModel {
        root_manifest,
        member_manifests,
        is_virtual,
        workspace_msrv,
        workspace_edition,
        workspace_resolver,
        member_patterns,
        exclude_patterns,
    })
}

/// Parses a Cargo.toml manifest into a `ParsedManifest`.
fn parse_manifest(value: &toml::Value) -> Result<ParsedManifest> {
    let package_name = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let (rust_version, rust_version_workspace) =
        parse_package_inheritable_string(value, "rust-version")?;
    let (edition, edition_workspace) = parse_package_inheritable_string(value, "edition")?;

    Ok(ParsedManifest {
        value: value.clone(),
        package_name,
        rust_version,
        rust_version_workspace,
        edition,
        edition_workspace,
    })
}

/// Extracts workspace-level settings from a root manifest.
fn extract_workspace_settings(
    value: &toml::Value,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let workspace_msrv = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(|rv| rv.as_str())
        .map(|s| s.to_string());

    let workspace_edition = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("edition"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    let workspace_resolver = value
        .get("workspace")
        .and_then(|w| w.get("resolver"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    // Also check for non-workspace package MSRV as fallback
    let package_msrv = value
        .get("package")
        .and_then(|p| p.get("rust-version"))
        .and_then(|rv| rv.as_str())
        .map(|s| s.to_string());

    let msrv = workspace_msrv.or(package_msrv);

    Ok((msrv, workspace_edition, workspace_resolver))
}

/// Extracts a string array from a nested path in a TOML value.
fn extract_string_array(value: &toml::Value, path: &[&str]) -> Vec<String> {
    let mut current = value;
    for key in path {
        match current.get(*key) {
            Some(v) => current = v,
            None => return Vec::new(),
        }
    }

    current
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Expands workspace member patterns and filters by exclude patterns.
///
/// Returns a sorted, deduplicated list of member paths (directories containing Cargo.toml).
fn expand_workspace_patterns(
    workspace_root: &Utf8Path,
    member_patterns: &[String],
    exclude_patterns: &[String],
) -> Result<Vec<String>> {
    // Build exclude globset
    let mut exclude_builder = GlobSetBuilder::new();
    for pattern in exclude_patterns {
        let glob = Glob::new(pattern)
            .with_context(|| format!("invalid exclude glob pattern: {pattern}"))?;
        exclude_builder.add(glob);
    }
    let exclude_set = exclude_builder
        .build()
        .context("failed to build exclude globset")?;

    // Expand each member pattern
    let mut discovered: BTreeSet<String> = BTreeSet::new();

    for pattern in member_patterns {
        let expanded = expand_glob_pattern(workspace_root, pattern)?;
        for path in expanded {
            // Check if excluded
            if !exclude_set.is_match(&path) {
                discovered.insert(path);
            }
        }
    }

    // Return sorted vec (BTreeSet already sorts)
    Ok(discovered.into_iter().collect())
}

/// Expands a single glob pattern relative to the workspace root.
///
/// Returns paths that:
/// 1. Match the glob pattern
/// 2. Contain a Cargo.toml file (are valid Cargo packages)
fn expand_glob_pattern(workspace_root: &Utf8Path, pattern: &str) -> Result<Vec<String>> {
    let normalized_pattern = normalize_slashes(pattern);

    // Check if pattern contains glob characters
    if !contains_glob_chars(&normalized_pattern) {
        // No glob, just return the pattern if it's a valid package directory
        let candidate = workspace_root.join(&normalized_pattern);
        if candidate.join("Cargo.toml").exists() {
            return Ok(vec![normalized_pattern]);
        }
        return Ok(Vec::new());
    }

    // Split pattern into base path (no globs) and glob part
    let (base_path, glob_pattern) = split_glob_pattern(&normalized_pattern);

    let search_root = if base_path.is_empty() {
        workspace_root.to_owned()
    } else {
        workspace_root.join(&base_path)
    };

    if !search_root.exists() {
        return Ok(Vec::new());
    }

    // Build glob matcher
    let glob = Glob::new(&glob_pattern)
        .with_context(|| format!("invalid glob pattern: {glob_pattern}"))?
        .compile_matcher();

    // Walk the directory tree
    let mut results = Vec::new();
    walk_for_cargo_tomls(&search_root, &base_path, &glob, &mut results)?;

    Ok(results)
}

/// Checks if a string contains glob metacharacters.
fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

/// Splits a glob pattern into a base path (no globs) and the glob portion.
fn split_glob_pattern(pattern: &str) -> (String, String) {
    let parts: Vec<&str> = pattern.split('/').collect();
    let mut base_parts = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        if contains_glob_chars(part) {
            // Return base and remainder
            let base = base_parts.join("/");
            let glob = parts[i..].join("/");
            return (base, glob);
        }
        base_parts.push(*part);
    }

    // No glob found, entire pattern is base
    (pattern.to_string(), String::new())
}

/// Recursively walks directories looking for Cargo.toml files that match the glob.
fn walk_for_cargo_tomls(
    current_dir: &Utf8Path,
    relative_prefix: &str,
    glob: &globset::GlobMatcher,
    results: &mut Vec<String>,
) -> Result<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // Skip directories we can't read
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let utf8_path =
            Utf8PathBuf::from_path_buf(path.clone()).expect("non-utf8 path in workspace walk");
        let dir_name = utf8_path
            .file_name()
            .expect("missing directory name in workspace walk")
            .to_string();
        // Build relative path
        let relative = if relative_prefix.is_empty() {
            dir_name
        } else {
            format!("{}/{}", relative_prefix, dir_name)
        };

        // Check if this directory matches the glob and has a Cargo.toml
        if glob.is_match(&relative) && utf8_path.join("Cargo.toml").exists() {
            results.push(relative.clone());
        }

        // Recurse into subdirectories (needed for patterns like "crates/**")
        walk_for_cargo_tomls(&utf8_path, &relative, glob, results)?;
    }

    Ok(())
}

fn parse_checksums(path: &Utf8Path) -> Result<ToolsChecksums> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let entries = parse_checksums_content(&txt);

    Ok(ToolsChecksums {
        path: path.to_path_buf(),
        entries,
    })
}

/// Parses checksums content from a string.
///
/// This function parses checksums in the standard format used by sha256sum:
/// `<hash><whitespace><path>` per line.
///
/// - Empty lines are ignored
/// - Lines starting with `#` are treated as comments and ignored
/// - Hash-only lines (no path) are parsed with an empty path
///
/// This is exposed publicly for fuzz testing. The main entry point for
/// production use is [`load_repo_state`] which reads from a file.
///
/// # Examples
///
/// ```
/// use builddiag_repo::parse_checksums_content;
///
/// let content = "abc123  path/to/file.txt\n# comment\ndef456  another/file.bin";
/// let entries = parse_checksums_content(content);
///
/// assert_eq!(entries.len(), 2);
/// assert_eq!(entries[0].hash, "abc123");
/// assert_eq!(entries[0].path, "path/to/file.txt");
/// assert_eq!(entries[0].line, 1);
/// assert_eq!(entries[1].line, 3);
/// ```
pub fn parse_checksums_content(content: &str) -> Vec<ChecksumEntry> {
    let mut entries = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // common format: <hash><space(s)><path>
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next().unwrap().to_string();
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

    entries
}

/// A helper for checks that need normalized versions.
///
/// Returns `Ok(None)` if input is non-numeric (stable/beta/nightly).
pub fn maybe_parse_numeric_version(s: &str) -> Result<Option<String>> {
    let t = s.trim();
    let base = t.split_once("-").map(|(a, _)| a).unwrap_or(t);
    if base.eq_ignore_ascii_case("stable")
        || base.eq_ignore_ascii_case("beta")
        || base.eq_ignore_ascii_case("nightly")
    {
        return Ok(None);
    }
    // Accept and normalize; this will error for invalid.
    let v = parse_rust_version(base)?;
    Ok(Some(v.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Tests for parse_checksums (Task 7.1)
    // Validates: Requirements 5.4
    // =========================================================================

    #[test]
    fn parse_checksums_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        let checksums_content = "\
abc123def456  path/to/file1.txt
789xyz000111  path/to/file2.bin
";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].line, 1);
        assert_eq!(result.entries[0].hash, "abc123def456");
        assert_eq!(result.entries[0].path, "path/to/file1.txt");
        assert_eq!(result.entries[1].line, 2);
        assert_eq!(result.entries[1].hash, "789xyz000111");
        assert_eq!(result.entries[1].path, "path/to/file2.bin");
    }

    #[test]
    fn parse_checksums_handles_comments() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        let checksums_content = "\
# This is a comment
abc123def456  path/to/file1.txt
# Another comment
789xyz000111  path/to/file2.bin
";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.entries.len(), 2);
        // Line numbers should account for comments
        assert_eq!(result.entries[0].line, 2);
        assert_eq!(result.entries[0].hash, "abc123def456");
        assert_eq!(result.entries[1].line, 4);
        assert_eq!(result.entries[1].hash, "789xyz000111");
    }

    #[test]
    fn parse_checksums_handles_empty_lines() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        let checksums_content = "\
abc123def456  path/to/file1.txt

789xyz000111  path/to/file2.bin

";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].line, 1);
        assert_eq!(result.entries[1].line, 3);
    }

    #[test]
    fn parse_checksums_handles_mixed_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        // Multiple spaces and tabs between hash and path
        let checksums_content =
            "abc123def456    path/to/file1.txt\n789xyz000111\t\tpath/to/file2.bin\n";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].hash, "abc123def456");
        assert_eq!(result.entries[0].path, "path/to/file1.txt");
        assert_eq!(result.entries[1].hash, "789xyz000111");
        assert_eq!(result.entries[1].path, "path/to/file2.bin");
    }

    #[test]
    fn parse_checksums_handles_hash_only_line() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        // Line with only a hash (no path)
        let checksums_content = "abc123def456\n789xyz000111  path/to/file.txt\n";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].hash, "abc123def456");
        assert_eq!(result.entries[0].path, ""); // Empty path for hash-only line
        assert_eq!(result.entries[1].hash, "789xyz000111");
        assert_eq!(result.entries[1].path, "path/to/file.txt");
    }

    #[test]
    fn parse_checksums_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        std::fs::write(&checksums_path, "").unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert!(result.entries.is_empty());
    }

    #[test]
    fn parse_checksums_only_comments_and_empty_lines() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        let checksums_content = "\
# Comment 1
# Comment 2

# Comment 3
";
        std::fs::write(&checksums_path, checksums_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert!(result.entries.is_empty());
    }

    #[test]
    fn parse_checksums_preserves_path() {
        let temp_dir = TempDir::new().unwrap();
        let checksums_path = temp_dir.path().join("checksums.txt");
        std::fs::write(&checksums_path, "abc123  file.txt\n").unwrap();
        let path = Utf8PathBuf::from_path_buf(checksums_path.clone()).unwrap();

        let result = parse_checksums(&path).unwrap();

        assert_eq!(result.path, path);
    }

    // =========================================================================
    // Tests for toolchain file parsing (Task 7.2)
    // Validates: Requirements 5.4
    // =========================================================================

    #[test]
    fn parse_rust_toolchain_toml_standard_format() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        let toolchain_content = r#"
    [toolchain]
    channel = "1.92.0"
    "#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path).unwrap();

        assert_eq!(result, "1.92.0");
    }

    #[test]
    fn parse_rust_toolchain_toml_with_components() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        let toolchain_content = r#"
[toolchain]
channel = "1.70.0"
components = ["rustfmt", "clippy"]
targets = ["x86_64-unknown-linux-gnu"]
"#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path).unwrap();

        assert_eq!(result, "1.70.0");
    }

    #[test]
    fn parse_rust_toolchain_toml_stable_channel() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        let toolchain_content = r#"
[toolchain]
channel = "stable"
"#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path).unwrap();

        assert_eq!(result, "stable");
    }

    #[test]
    fn parse_rust_toolchain_toml_nightly_channel() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        let toolchain_content = r#"
[toolchain]
channel = "nightly-2024-01-15"
"#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path).unwrap();

        assert_eq!(result, "nightly-2024-01-15");
    }

    #[test]
    fn parse_rust_toolchain_toml_fallback_top_level_channel() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        // Non-standard format with top-level channel (fallback behavior)
        let toolchain_content = r#"
channel = "1.72.0"
"#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path).unwrap();

        assert_eq!(result, "1.72.0");
    }

    #[test]
    fn parse_rust_toolchain_toml_missing_channel_fails() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        let toolchain_content = r#"
[toolchain]
components = ["rustfmt"]
"#;
        std::fs::write(&toolchain_path, toolchain_content).unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing toolchain.channel"));
    }

    #[test]
    fn parse_rust_toolchain_toml_empty_file_fails() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain.toml");
        std::fs::write(&toolchain_path, "").unwrap();
        let path = Utf8PathBuf::from_path_buf(toolchain_path).unwrap();

        let result = parse_rust_toolchain_toml(&path);

        assert!(result.is_err());
    }

    // =========================================================================
    // Tests for legacy rust-toolchain format (Task 7.2)
    // Validates: Requirements 5.4
    // =========================================================================

    #[test]
    fn find_toolchain_legacy_format() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain");
        std::fs::write(&toolchain_path, "1.70.0\n").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml").unwrap();

        assert!(result.is_some());
        let toolchain = result.unwrap();
        assert_eq!(toolchain.channel, "1.70.0");
        assert!(toolchain.path.ends_with("rust-toolchain"));
    }

    #[test]
    fn find_toolchain_legacy_format_stable() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain");
        std::fs::write(&toolchain_path, "stable\n").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml").unwrap();

        assert!(result.is_some());
        let toolchain = result.unwrap();
        assert_eq!(toolchain.channel, "stable");
    }

    #[test]
    fn find_toolchain_legacy_format_with_trailing_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain");
        std::fs::write(&toolchain_path, "  1.70.0  \n").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml").unwrap();

        assert!(result.is_some());
        let toolchain = result.unwrap();
        assert_eq!(toolchain.channel, "1.70.0");
    }

    #[test]
    fn find_toolchain_legacy_format_empty_fails() {
        let temp_dir = TempDir::new().unwrap();
        let toolchain_path = temp_dir.path().join("rust-toolchain");
        std::fs::write(&toolchain_path, "").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("empty"));
    }

    #[test]
    fn find_toolchain_prefers_toml_over_legacy() {
        let temp_dir = TempDir::new().unwrap();
        // Create both files
        let toml_path = temp_dir.path().join("rust-toolchain.toml");
        let legacy_path = temp_dir.path().join("rust-toolchain");
        std::fs::write(&toml_path, "[toolchain]\nchannel = \"1.92.0\"\n").unwrap();
        std::fs::write(&legacy_path, "1.70.0\n").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml").unwrap();

        assert!(result.is_some());
        let toolchain = result.unwrap();
        // Should prefer the TOML format
        assert_eq!(toolchain.channel, "1.92.0");
        assert!(toolchain.path.ends_with("rust-toolchain.toml"));
    }

    #[test]
    fn find_toolchain_no_toolchain_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let result = find_toolchain(&root, "rust-toolchain.toml").unwrap();

        assert!(result.is_none());
    }

    // =========================================================================
    // Tests for glob pattern helpers
    // =========================================================================

    #[test]
    fn contains_glob_chars_detects_asterisk() {
        assert!(contains_glob_chars("crates/*"));
        assert!(contains_glob_chars("**/*.rs"));
    }

    #[test]
    fn contains_glob_chars_detects_question_mark() {
        assert!(contains_glob_chars("file?.txt"));
    }

    #[test]
    fn contains_glob_chars_detects_brackets() {
        assert!(contains_glob_chars("file[0-9].txt"));
    }

    #[test]
    fn contains_glob_chars_detects_braces() {
        assert!(contains_glob_chars("{foo,bar}"));
    }

    #[test]
    fn contains_glob_chars_returns_false_for_plain_path() {
        assert!(!contains_glob_chars("crates/foo/bar"));
        assert!(!contains_glob_chars("simple.txt"));
    }

    #[test]
    fn split_glob_pattern_with_glob_in_middle() {
        let (base, glob) = split_glob_pattern("crates/*/src");
        assert_eq!(base, "crates");
        assert_eq!(glob, "*/src");
    }

    #[test]
    fn split_glob_pattern_with_glob_at_start() {
        let (base, glob) = split_glob_pattern("*/foo/bar");
        assert_eq!(base, "");
        assert_eq!(glob, "*/foo/bar");
    }

    #[test]
    fn split_glob_pattern_with_no_glob() {
        let (base, glob) = split_glob_pattern("crates/foo/bar");
        assert_eq!(base, "crates/foo/bar");
        assert_eq!(glob, "");
    }

    #[test]
    fn expand_glob_pattern_returns_empty_for_missing_path() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let expanded = expand_glob_pattern(&root, "missing/path").unwrap();
        assert!(expanded.is_empty());
    }

    #[test]
    fn expand_glob_pattern_handles_base_path_empty() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let member_dir = root.join("member");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(member_dir.join("Cargo.toml"), make_package_toml("member")).unwrap();

        let expanded = expand_glob_pattern(&root, "*").unwrap();
        assert!(expanded.contains(&"member".to_string()));
    }

    #[test]
    fn expand_glob_pattern_returns_empty_for_missing_search_root() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let expanded = expand_glob_pattern(&root, "missing/*").unwrap();
        assert!(expanded.is_empty());
    }

    #[test]
    fn walk_for_cargo_tomls_skips_unreadable_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let file_path = root.join("not-a-dir");
        std::fs::write(&file_path, "data").unwrap();

        let glob = globset::Glob::new("*").unwrap().compile_matcher();
        let mut results = Vec::new();
        walk_for_cargo_tomls(&file_path, "", &glob, &mut results).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn walk_for_cargo_tomls_uses_empty_relative_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let member_dir = root.join("member");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(member_dir.join("Cargo.toml"), make_package_toml("member")).unwrap();

        let glob = globset::Glob::new("*").unwrap().compile_matcher();
        let mut results = Vec::new();
        walk_for_cargo_tomls(&root, "", &glob, &mut results).unwrap();
        assert!(results.contains(&"member".to_string()));
    }

    // =========================================================================
    // Tests for workspace discovery
    // =========================================================================

    /// Helper to create a minimal Cargo.toml content for a package.
    fn make_package_toml(name: &str) -> String {
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#
        )
    }

    fn create_minimal_repo() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), make_package_toml("fixture")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        (temp, root)
    }

    /// Helper to create a workspace Cargo.toml with members.
    fn make_workspace_toml(members: &[&str]) -> String {
        let members_str: Vec<String> = members.iter().map(|m| format!("\"{}\"", m)).collect();
        format!(
            r#"[workspace]
resolver = "2"
members = [
    {}
]
"#,
            members_str.join(",\n    ")
        )
    }

    #[test]
    fn discover_workspace_single_crate() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("Cargo.toml");
        std::fs::write(&manifest_path, make_package_toml("my-crate")).unwrap();
        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        let model = discover_workspace(&path).unwrap();

        assert!(!model.is_virtual);
        assert_eq!(model.member_manifests.len(), 1);
        assert!(model.member_manifests.contains_key("Cargo.toml"));
        assert!(model.member_patterns.is_empty());
        assert!(model.exclude_patterns.is_empty());
    }

    #[test]
    fn discover_workspace_with_explicit_members() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create root Cargo.toml
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(
            &manifest_path,
            make_workspace_toml(&["crates/foo", "crates/bar"]),
        )
        .unwrap();

        // Create member directories
        std::fs::create_dir_all(root.join("crates/foo")).unwrap();
        std::fs::create_dir_all(root.join("crates/bar")).unwrap();

        // Create member Cargo.tomls
        std::fs::write(root.join("crates/foo/Cargo.toml"), make_package_toml("foo")).unwrap();
        std::fs::write(root.join("crates/bar/Cargo.toml"), make_package_toml("bar")).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        assert!(model.is_virtual);
        assert_eq!(model.member_manifests.len(), 2);
        assert!(model.member_manifests.contains_key("crates/bar/Cargo.toml"));
        assert!(model.member_manifests.contains_key("crates/foo/Cargo.toml"));

        // Verify deterministic ordering (bar comes before foo alphabetically)
        let paths: Vec<&str> = model.member_manifests.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            paths,
            vec!["crates/bar/Cargo.toml", "crates/foo/Cargo.toml"]
        );
    }

    #[test]
    fn load_workspace_skips_non_member_packages() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let dep_dir = TempDir::new().unwrap();
        let dep_root = dep_dir.path();

        let dep_path = dep_root.to_string_lossy().replace('\\', "/");
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "root"
version = "0.1.0"
edition = "2021"

[workspace]
members = ["crates/a"]

[dependencies]
dep = {{ path = "{dep_path}" }}
"#
            ),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::write(root.join("crates/a/Cargo.toml"), make_package_toml("a")).unwrap();
        std::fs::write(root.join("crates/a/src/lib.rs"), "").unwrap();

        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::write(dep_root.join("Cargo.toml"), make_package_toml("dep")).unwrap();
        std::fs::write(dep_root.join("src/lib.rs"), "").unwrap();

        let manifest_path = Utf8PathBuf::from_path_buf(root.join("Cargo.toml")).unwrap();
        let workspace = load_workspace(&manifest_path).unwrap();
        assert!(workspace.members.iter().any(|m| m.name == "a"));
        assert!(!workspace.members.iter().any(|m| m.name == "dep"));
    }

    #[test]
    fn discover_workspace_with_glob_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create root Cargo.toml with glob pattern
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create member directories
        std::fs::create_dir_all(root.join("crates/alpha")).unwrap();
        std::fs::create_dir_all(root.join("crates/beta")).unwrap();
        std::fs::create_dir_all(root.join("crates/gamma")).unwrap();

        // Create member Cargo.tomls
        std::fs::write(
            root.join("crates/alpha/Cargo.toml"),
            make_package_toml("alpha"),
        )
        .unwrap();
        std::fs::write(
            root.join("crates/beta/Cargo.toml"),
            make_package_toml("beta"),
        )
        .unwrap();
        std::fs::write(
            root.join("crates/gamma/Cargo.toml"),
            make_package_toml("gamma"),
        )
        .unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        assert!(model.is_virtual);
        assert_eq!(model.member_manifests.len(), 3);

        // Verify deterministic ordering (alphabetical)
        let paths: Vec<&str> = model.member_manifests.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "crates/alpha/Cargo.toml",
                "crates/beta/Cargo.toml",
                "crates/gamma/Cargo.toml"
            ]
        );
    }

    #[test]
    fn discover_workspace_skips_missing_member_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        std::fs::create_dir_all(root.join("crates/empty")).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();
        assert!(
            !model
                .member_manifests
                .contains_key("crates/empty/Cargo.toml")
        );
    }

    #[test]
    fn discover_workspace_with_exclude_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create root Cargo.toml with exclude
        let manifest_content = r#"[workspace]
resolver = "2"
members = ["crates/*"]
exclude = ["crates/excluded"]
"#;
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, manifest_content).unwrap();

        // Create member directories
        std::fs::create_dir_all(root.join("crates/included")).unwrap();
        std::fs::create_dir_all(root.join("crates/excluded")).unwrap();

        // Create member Cargo.tomls
        std::fs::write(
            root.join("crates/included/Cargo.toml"),
            make_package_toml("included"),
        )
        .unwrap();
        std::fs::write(
            root.join("crates/excluded/Cargo.toml"),
            make_package_toml("excluded"),
        )
        .unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        // Should only have the included member
        assert_eq!(model.member_manifests.len(), 1);
        assert!(
            model
                .member_manifests
                .contains_key("crates/included/Cargo.toml")
        );
        assert!(
            !model
                .member_manifests
                .contains_key("crates/excluded/Cargo.toml")
        );
    }

    #[test]
    fn discover_workspace_non_virtual_includes_root_package() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create root Cargo.toml with both package and workspace
        let manifest_content = r#"[package]
name = "root-pkg"
version = "0.1.0"
edition = "2021"

[workspace]
resolver = "2"
members = ["crates/sub"]
"#;
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, manifest_content).unwrap();

        // Create sub-crate
        std::fs::create_dir_all(root.join("crates/sub")).unwrap();
        std::fs::write(root.join("crates/sub/Cargo.toml"), make_package_toml("sub")).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        // Should not be virtual (has package)
        assert!(!model.is_virtual);
        // Should include both root and sub
        assert_eq!(model.member_manifests.len(), 2);
        assert!(model.member_manifests.contains_key("Cargo.toml"));
        assert!(model.member_manifests.contains_key("crates/sub/Cargo.toml"));
    }

    #[test]
    fn discover_workspace_extracts_workspace_msrv() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let manifest_content = r#"[workspace]
resolver = "2"
members = []

[workspace.package]
rust-version = "1.70.0"
edition = "2021"
"#;
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, manifest_content).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        assert_eq!(model.workspace_msrv, Some("1.70.0".to_string()));
        assert_eq!(model.workspace_edition, Some("2021".to_string()));
        assert_eq!(model.workspace_resolver, Some("2".to_string()));
    }

    #[test]
    fn discover_workspace_deterministic_ordering() {
        // Run discovery twice and verify same order
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create members in "random" order
        for name in ["zeta", "alpha", "mango", "beta", "delta"] {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            )
            .unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        // Run discovery multiple times
        let model1 = discover_workspace(&path).unwrap();
        let model2 = discover_workspace(&path).unwrap();

        // Should have same order (alphabetical)
        let paths1: Vec<&str> = model1.member_manifests.keys().map(|s| s.as_str()).collect();
        let paths2: Vec<&str> = model2.member_manifests.keys().map(|s| s.as_str()).collect();

        assert_eq!(paths1, paths2);
        assert_eq!(
            paths1,
            vec![
                "crates/alpha/Cargo.toml",
                "crates/beta/Cargo.toml",
                "crates/delta/Cargo.toml",
                "crates/mango/Cargo.toml",
                "crates/zeta/Cargo.toml"
            ]
        );
    }

    #[test]
    fn workspace_model_methods() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("Cargo.toml");
        std::fs::write(&manifest_path, make_package_toml("test")).unwrap();
        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        let model = discover_workspace(&path).unwrap();

        assert!(model.has_members());
        assert_eq!(model.member_count(), 1);
        assert_eq!(model.member_paths(), vec!["Cargo.toml"]);
    }

    // =========================================================================
    // Additional coverage for repo loading and metadata parsing
    // =========================================================================

    #[test]
    fn load_repo_state_handles_missing_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8Path::from_path(temp_dir.path()).unwrap();

        let state = load_repo_state(root, &Config::default(), None).unwrap();
        assert!(state.cargo_root.is_none());
        assert!(!state.workspace.is_workspace);
        assert!(!state.lockfile_exists);
    }

    #[test]
    fn load_repo_state_reads_toolchain_and_tools() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(
            root.join("rust-toolchain.toml"),
            r#"
[toolchain]
channel = "1.70.0"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join("scripts/tools.sha256"),
            format!("{}  tool.bin\n", "a".repeat(64)),
        )
        .unwrap();
        std::fs::write(
            root.join("scripts/tools.toml"),
            r#"
[[tool]]
name = "tools"
files = ["tool.bin"]
"#,
        )
        .unwrap();
        std::fs::write(root.join("Cargo.lock"), "").unwrap();

        let state = load_repo_state(&root, &Config::default(), None).unwrap();
        assert!(state.cargo_root.is_some());
        assert!(state.toolchain.is_some());
        assert_eq!(state.toolchain.as_ref().unwrap().channel, "1.70.0");
        assert!(state.tools_checksums.is_some());
        assert_eq!(state.tools_checksums.as_ref().unwrap().entries.len(), 1);
        assert!(state.tools_manifest.is_some());
        assert!(state.lockfile_exists);
    }

    #[test]
    fn load_repo_state_cached_respects_disabled_config() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        let config = Config::default();
        let state = load_repo_state_cached(&root, &config, None, None).unwrap();
        assert!(state.cargo_root.is_some());

        let disabled = CacheConfig::disabled();
        let state = load_repo_state_cached(&root, &config, None, Some(&disabled)).unwrap();
        assert!(state.cargo_root.is_some());
    }

    #[test]
    fn load_repo_state_cached_handles_cache_save_failure() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        let cache_file = root.join("cache-file");
        std::fs::write(&cache_file, "not a dir").unwrap();
        let cache_cfg = CacheConfig {
            cache_dir: cache_file,
            ..Default::default()
        };

        let config = Config::default();
        let state = load_repo_state_cached(&root, &config, None, Some(&cache_cfg)).unwrap();
        assert!(state.cargo_root.is_some());
    }

    #[test]
    fn load_repo_state_cached_writes_cache_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/a"]

[workspace.package]
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            r#"
[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(root.join("crates/a/src/lib.rs"), "").unwrap();
        std::fs::write(
            root.join("rust-toolchain.toml"),
            r#"
[toolchain]
channel = "1.70.0"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join("scripts/tools.sha256"),
            format!("{}  tool.bin\n", "a".repeat(64)),
        )
        .unwrap();
        std::fs::write(
            root.join("scripts/tools.toml"),
            r#"
[[tool]]
name = "tools"
files = ["tool.bin"]
"#,
        )
        .unwrap();
        std::fs::write(root.join("Cargo.lock"), "").unwrap();

        let config = Config::default();
        let cache_cfg = CacheConfig::default();
        let _state = load_repo_state_cached(&root, &config, None, Some(&cache_cfg)).unwrap();

        let cache_path = cache_cfg.cache_dir_abs(&root).join("repo-state.json");
        assert!(cache_path.exists());
    }

    #[cfg(feature = "cache")]
    #[test]
    fn build_cache_from_state_records_cargo_root() {
        let (_temp, root) = create_minimal_repo();
        let config = Config::default();
        let state = load_repo_state(&root, &config, None).unwrap();

        let cache = build_cache_from_state(&root, &state, &config).unwrap();
        assert!(cache.cargo_root.meta.is_some());
    }

    #[test]
    fn repo_state_from_substrate_builds_members() {
        let root = Utf8Path::new("/repo");
        let substrate = builddiag_types::Substrate {
            manifests: vec![builddiag_types::ManifestInfo {
                path: "crates/demo/Cargo.toml".to_string(),
                name: Some("demo".to_string()),
                msrv: Some("1.70.0".to_string()),
                edition: Some("2021".to_string()),
            }],
            has_toolchain: true,
            toolchain_channel: Some("1.70.0".to_string()),
            has_checksums: false,
            has_lockfile: true,
            workspace_msrv: Some("1.70.0".to_string()),
        };

        let state = repo_state_from_substrate(root, &substrate);
        assert!(state.toolchain.is_some());
        assert_eq!(state.workspace.members.len(), 1);
        assert!(state.lockfile_exists);
    }

    #[test]
    fn repo_state_from_substrate_without_toolchain() {
        let root = Utf8Path::new("/repo");
        let substrate = builddiag_types::Substrate {
            manifests: vec![],
            has_toolchain: false,
            toolchain_channel: None,
            has_checksums: false,
            has_lockfile: false,
            workspace_msrv: None,
        };

        let state = repo_state_from_substrate(root, &substrate);
        assert!(state.toolchain.is_none());
    }

    #[test]
    fn has_binary_target_detects_bin_and_main() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let manifest_path = root.join("Cargo.toml");

        let manifest_bin = r#"
[package]
name = "bin-test"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "app"
"#;
        let value: toml::Value = toml::from_str(manifest_bin).unwrap();
        std::fs::write(&manifest_path, manifest_bin).unwrap();
        assert!(has_binary_target(&manifest_path, &value).unwrap());

        let manifest_no_bin = r#"
[package]
name = "bin-test"
version = "0.1.0"
edition = "2021"
"#;
        let value: toml::Value = toml::from_str(manifest_no_bin).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        assert!(has_binary_target(&manifest_path, &value).unwrap());

        std::fs::remove_file(root.join("src/main.rs")).unwrap();
        assert!(!has_binary_target(&manifest_path, &value).unwrap());
    }

    #[test]
    fn parse_publish_metadata_captures_fields() {
        let manifest = r#"
[package]
name = "meta"
version = "0.1.0"
publish = false
description = "desc"
license = "MIT"
license-file = "LICENSE"
repository = "https://example.com/repo"
homepage = "https://example.com"
documentation = "https://docs.example.com"
readme = "README.md"
keywords = ["a", "b"]
categories = ["development-tools"]
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let meta = parse_publish_metadata(&value);

        assert!(meta.publish_disabled);
        assert_eq!(meta.description.as_deref(), Some("desc"));
        assert_eq!(meta.license.as_deref(), Some("MIT"));
        assert_eq!(meta.license_file.as_deref(), Some("LICENSE"));
        assert_eq!(meta.repository.as_deref(), Some("https://example.com/repo"));
        assert_eq!(meta.homepage.as_deref(), Some("https://example.com"));
        assert_eq!(
            meta.documentation.as_deref(),
            Some("https://docs.example.com")
        );
        assert_eq!(meta.readme.as_deref(), Some("README.md"));
        assert_eq!(meta.keywords, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(meta.categories, vec!["development-tools".to_string()]);
    }

    #[test]
    fn parse_publish_metadata_defaults_without_package() {
        let value: toml::Value = toml::from_str("").unwrap();
        let meta = parse_publish_metadata(&value);
        assert!(!meta.publish_disabled);
        assert!(meta.description.is_none());
    }

    #[test]
    fn parse_publish_metadata_disables_publish_for_empty_array() {
        let manifest = r#"
[package]
name = "meta"
version = "0.1.0"
publish = []
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let meta = parse_publish_metadata(&value);
        assert!(meta.publish_disabled);
    }

    #[test]
    fn parse_package_inheritable_string_rejects_unsupported_type() {
        let manifest = r#"
[package]
name = "demo"
version = "0.1.0"
rust-version = 1
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let err = parse_package_inheritable_string(&value, "rust-version").unwrap_err();
        assert!(err.to_string().contains("unsupported type"));
    }

    // =========================================================================
    // Error-path coverage tests
    // =========================================================================

    #[test]
    fn load_repo_state_errors_on_nonexistent_root() {
        // Canonicalize fails when the path does not exist.
        let bogus = Utf8Path::new("/this/path/should/not/exist/builddiag-test-xyz");
        let result = load_repo_state(bogus, &Config::default(), None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("canonicalize"));
    }

    #[test]
    fn load_repo_state_errors_on_malformed_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // Write a malformed Cargo.toml.
        std::fs::write(root.join("Cargo.toml"), "not [valid toml === broken").unwrap();

        let result = load_repo_state(&root, &Config::default(), None);
        assert!(result.is_err());
    }

    #[test]
    fn load_repo_state_propagates_tools_manifest_parse_error() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        std::fs::write(root.join("Cargo.toml"), make_package_toml("demo")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        // Broken tools.toml so that toml::from_str fails.
        std::fs::write(
            root.join("scripts/tools.toml"),
            "this is = not valid toml ===",
        )
        .unwrap();

        let result = load_repo_state(&root, &Config::default(), None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("parse tools manifest") || err_msg.contains("tools.toml"));
    }

    #[test]
    fn parse_rust_toolchain_toml_errors_for_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let missing = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .unwrap()
            .join("missing-toolchain.toml");
        let result = parse_rust_toolchain_toml(&missing);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("read"));
    }

    #[test]
    fn parse_rust_toolchain_toml_errors_for_malformed_toml() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("rust-toolchain.toml");
        std::fs::write(&path, "this is === not valid toml").unwrap();
        let p = Utf8PathBuf::from_path_buf(path).unwrap();

        let result = parse_rust_toolchain_toml(&p);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("parse"));
    }

    #[test]
    fn parse_rust_toolchain_toml_errors_when_no_channel_anywhere() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("rust-toolchain.toml");
        // Valid TOML but no toolchain.channel and no top-level channel.
        std::fs::write(&path, "[other]\nkey = \"value\"\n").unwrap();
        let p = Utf8PathBuf::from_path_buf(path).unwrap();

        let result = parse_rust_toolchain_toml(&p);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing toolchain.channel"));
    }

    #[test]
    fn find_toolchain_uses_legacy_when_only_legacy_exists() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("rust-toolchain"), "1.65.0\n").unwrap();
        let tc = find_toolchain(&root, "rust-toolchain.toml")
            .unwrap()
            .unwrap();
        assert_eq!(tc.channel, "1.65.0");
        assert!(tc.path.as_str().ends_with("rust-toolchain"));
    }

    #[test]
    fn metadata_errors_for_non_workspace_path() {
        let temp_dir = TempDir::new().unwrap();
        // Path doesn't exist or is not a Cargo manifest.
        let manifest_path = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .unwrap()
            .join("not-a-manifest.toml");
        let result = metadata(&manifest_path);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cargo metadata"));
    }

    #[test]
    fn load_workspace_errors_for_malformed_root_manifest() {
        // cargo_metadata will reject a malformed Cargo.toml.
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("Cargo.toml");
        std::fs::write(&manifest_path, "not [valid toml ===").unwrap();
        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        let result = load_workspace(&path);
        assert!(result.is_err());
    }

    #[test]
    fn parse_package_inheritable_string_returns_workspace_inheritance() {
        // Cover the Table branch (key.workspace = true).
        let manifest = r#"
[package]
name = "demo"
version = "0.1.0"
rust-version.workspace = true
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let (rv, inherits) = parse_package_inheritable_string(&value, "rust-version").unwrap();
        assert!(rv.is_none());
        assert!(inherits);
    }

    #[test]
    fn parse_package_inheritable_string_handles_table_without_workspace_key() {
        // Table syntax without `workspace = true` should give (None, false).
        let manifest = r#"
[package]
name = "demo"
version = "0.1.0"

[package.rust-version]
other = "value"
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let (rv, inherits) = parse_package_inheritable_string(&value, "rust-version").unwrap();
        assert!(rv.is_none());
        assert!(!inherits);
    }

    #[test]
    fn parse_package_inheritable_string_returns_none_without_package() {
        let value: toml::Value = toml::from_str("[other]\nkey = 1\n").unwrap();
        let (rv, inherits) = parse_package_inheritable_string(&value, "rust-version").unwrap();
        assert!(rv.is_none());
        assert!(!inherits);
    }

    #[test]
    fn parse_package_inheritable_string_returns_none_without_key() {
        let manifest = r#"
[package]
name = "demo"
version = "0.1.0"
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let (rv, inherits) = parse_package_inheritable_string(&value, "rust-version").unwrap();
        assert!(rv.is_none());
        assert!(!inherits);
    }

    #[test]
    fn parse_workspace_root_falls_back_to_package_msrv() {
        // No workspace section, but package has rust-version: function should
        // return that as msrv.
        let manifest = r#"
[package]
name = "x"
version = "0.1.0"
rust-version = "1.71.0"
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let (msrv, _edition, _resolver, is_workspace) = parse_workspace_root(&value).unwrap();
        assert_eq!(msrv.as_deref(), Some("1.71.0"));
        assert!(!is_workspace);
    }

    #[test]
    fn parse_workspace_root_extracts_workspace_settings() {
        let manifest = r#"
[workspace]
resolver = "2"
members = []

[workspace.package]
rust-version = "1.72.0"
edition = "2021"
"#;
        let value: toml::Value = toml::from_str(manifest).unwrap();
        let (msrv, edition, resolver, is_workspace) = parse_workspace_root(&value).unwrap();
        assert_eq!(msrv.as_deref(), Some("1.72.0"));
        assert_eq!(edition.as_deref(), Some("2021"));
        assert_eq!(resolver.as_deref(), Some("2"));
        assert!(is_workspace);
    }

    #[test]
    fn discover_workspace_errors_for_malformed_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("Cargo.toml");
        std::fs::write(&manifest_path, "not [valid toml ===").unwrap();
        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        let result = discover_workspace(&path);
        assert!(result.is_err());
    }

    #[test]
    fn discover_workspace_errors_for_missing_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("does-not-exist/Cargo.toml");
        let path = Utf8PathBuf::from_path_buf(missing).unwrap();

        let result = discover_workspace(&path);
        assert!(result.is_err());
    }

    #[test]
    fn discover_workspace_errors_when_member_manifest_unreadable() {
        // Create a workspace where the glob expansion includes a member dir
        // whose Cargo.toml is itself broken.
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/broken"])).unwrap();

        std::fs::create_dir_all(root.join("crates/broken")).unwrap();
        // Member manifest exists but is malformed.
        std::fs::write(root.join("crates/broken/Cargo.toml"), "not [valid toml ===").unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let result = discover_workspace(&path);
        assert!(result.is_err());
    }

    #[test]
    fn workspace_model_member_paths_returns_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let manifest_path = root.join("Cargo.toml");
        std::fs::write(
            &manifest_path,
            make_workspace_toml(&["crates/a", "crates/b"]),
        )
        .unwrap();

        std::fs::create_dir_all(root.join("crates/a")).unwrap();
        std::fs::create_dir_all(root.join("crates/b")).unwrap();
        std::fs::write(root.join("crates/a/Cargo.toml"), make_package_toml("a")).unwrap();
        std::fs::write(root.join("crates/b/Cargo.toml"), make_package_toml("b")).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        let paths = model.member_paths();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"crates/a/Cargo.toml"));
        assert!(paths.contains(&"crates/b/Cargo.toml"));
        assert!(model.has_members());
        assert_eq!(model.member_count(), 2);
    }

    #[test]
    fn expand_workspace_patterns_errors_on_invalid_exclude_glob() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        // `[` without a matching `]` is an invalid glob.
        let result = expand_workspace_patterns(&root, &[], &["[invalid".to_string()]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid exclude glob pattern"));
    }

    #[test]
    fn expand_glob_pattern_errors_on_invalid_glob() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // Create the base directory so the function reaches the glob construction.
        std::fs::create_dir_all(root.join("base")).unwrap();

        // Invalid character class glob inside the base directory.
        let result = expand_glob_pattern(&root, "base/[unterminated");
        assert!(result.is_err());
    }

    #[test]
    fn parse_checksums_errors_for_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let missing = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .unwrap()
            .join("missing.sha256");
        let result = parse_checksums(&missing);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("read"));
    }

    // =========================================================================
    // Tests for maybe_parse_numeric_version (uncovered helper)
    // =========================================================================

    #[test]
    fn maybe_parse_numeric_version_returns_none_for_stable() {
        assert!(maybe_parse_numeric_version("stable").unwrap().is_none());
        assert!(maybe_parse_numeric_version("STABLE").unwrap().is_none());
    }

    #[test]
    fn maybe_parse_numeric_version_returns_none_for_beta() {
        assert!(maybe_parse_numeric_version("beta").unwrap().is_none());
    }

    #[test]
    fn maybe_parse_numeric_version_returns_none_for_nightly() {
        assert!(maybe_parse_numeric_version("nightly").unwrap().is_none());
        assert!(
            maybe_parse_numeric_version("nightly-2024-01-15")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn maybe_parse_numeric_version_returns_normalized_version() {
        let v = maybe_parse_numeric_version("1.70.0").unwrap();
        assert_eq!(v.as_deref(), Some("1.70.0"));

        let v = maybe_parse_numeric_version("1.70").unwrap();
        // Should be parseable (parse_rust_version normalizes shorthand).
        assert!(v.is_some());
    }

    #[test]
    fn maybe_parse_numeric_version_errors_on_garbage() {
        let result = maybe_parse_numeric_version("not-a-version-xyz");
        assert!(result.is_err());
    }

    // =========================================================================
    // load_repo_state_cached coverage (feature = "cache")
    // =========================================================================

    #[cfg(feature = "cache")]
    #[test]
    fn load_repo_state_cached_hits_existing_cache_on_second_call() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("Cargo.toml"), make_package_toml("demo")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        let config = Config::default();
        let cache_cfg = CacheConfig::default();

        // First call: cache miss (no cache yet) -> writes cache.
        let _state1 = load_repo_state_cached(&root, &config, None, Some(&cache_cfg)).unwrap();
        let cache_file = cache_cfg.cache_dir_abs(&root).join("repo-state.json");
        assert!(cache_file.exists());

        // Second call: cache hit branch (load returns Some) executes successfully.
        let state2 = load_repo_state_cached(&root, &config, None, Some(&cache_cfg)).unwrap();
        assert!(state2.cargo_root.is_some());
    }

    #[cfg(feature = "cache")]
    #[test]
    fn load_repo_state_cached_with_corrupt_existing_cache_still_succeeds() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("Cargo.toml"), make_package_toml("demo")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();

        let config = Config::default();
        let cache_cfg = CacheConfig::default();
        let cache_dir = cache_cfg.cache_dir_abs(&root);
        std::fs::create_dir_all(&cache_dir).unwrap();
        // Corrupt cache file - implementation uses `.ok().flatten()` so it
        // silently treats this as a cache miss.
        std::fs::write(cache_dir.join("repo-state.json"), "{garbage").unwrap();

        let state = load_repo_state_cached(&root, &config, None, Some(&cache_cfg)).unwrap();
        assert!(state.cargo_root.is_some());
    }

    #[cfg(feature = "cache")]
    #[test]
    fn load_repo_state_cached_errors_on_nonexistent_root() {
        let bogus = Utf8Path::new("/definitely/not/a/real/path/builddiag-xyz");
        let config = Config::default();
        let cache_cfg = CacheConfig::default();
        let result = load_repo_state_cached(bogus, &config, None, Some(&cache_cfg));
        assert!(result.is_err());
    }
}
