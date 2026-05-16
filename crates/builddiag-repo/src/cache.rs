//! Repository state caching for incremental checking.
//!
//! This module provides caching for parsed repository state to avoid re-parsing
//! files that haven't changed. The cache uses file modification times and content
//! hashes to detect changes.
//!
//! # Cache Structure
//!
//! The cache is stored as a JSON file containing:
//! - File metadata (path, mtime, content hash)
//! - Parsed data for each file type
//!
//! # Cache Location
//!
//! By default, the cache is stored in `.builddiag-cache/` at the repository root.
//! This can be configured via the `cache_dir` option.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::time::SystemTime;

/// Default cache directory name.
pub const DEFAULT_CACHE_DIR: &str = ".builddiag-cache";

/// Cache file name.
const CACHE_FILE: &str = "repo-state.json";

/// Metadata about a cached file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    /// File path relative to repo root.
    pub path: String,
    /// File modification time as Unix timestamp (seconds since epoch).
    pub mtime: u64,
    /// SHA-256 hash of file contents.
    pub content_hash: String,
}

/// Cached data for the root Cargo.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedCargoRoot {
    /// File metadata.
    pub meta: Option<FileMeta>,
    /// Workspace MSRV.
    pub workspace_msrv: Option<String>,
    /// Workspace edition.
    pub workspace_edition: Option<String>,
    /// Workspace resolver.
    pub workspace_resolver: Option<String>,
    /// Whether this is a workspace.
    pub is_workspace: bool,
    /// Member patterns from [workspace.members].
    pub member_patterns: Vec<String>,
    /// Exclude patterns from [workspace.exclude].
    pub exclude_patterns: Vec<String>,
}

/// Cached data for rust-toolchain.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedToolchain {
    /// File metadata.
    pub meta: Option<FileMeta>,
    /// Parsed channel.
    pub channel: Option<String>,
}

/// Cached data for checksums file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedChecksums {
    /// File metadata.
    pub meta: Option<FileMeta>,
    /// Number of entries (for quick validation).
    pub entry_count: usize,
}

/// Cached data for a workspace member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMember {
    /// File metadata for the member's Cargo.toml.
    pub meta: FileMeta,
    /// Package name.
    pub name: String,
    /// Rust version if present.
    pub rust_version: Option<String>,
    /// Whether rust-version inherits from workspace.
    pub rust_version_workspace: bool,
    /// Edition if present.
    pub edition: Option<String>,
    /// Whether edition inherits from workspace.
    pub edition_workspace: bool,
    /// Whether this member has binary targets.
    pub has_binary_target: bool,
}

/// The complete repository state cache.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoStateCache {
    /// Cache format version for compatibility checking.
    pub version: u32,
    /// Cached root Cargo.toml data.
    pub cargo_root: CachedCargoRoot,
    /// Cached rust-toolchain.toml data.
    pub toolchain: CachedToolchain,
    /// Cached checksums data.
    pub checksums: CachedChecksums,
    /// Cached workspace members (path -> data).
    pub members: BTreeMap<String, CachedMember>,
    /// Whether Cargo.lock exists.
    pub lockfile_exists: bool,
}

/// Current cache format version.
const CACHE_VERSION: u32 = 1;

impl RepoStateCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            version: CACHE_VERSION,
            ..Default::default()
        }
    }

    /// Load cache from disk.
    pub fn load(cache_dir: &Utf8Path) -> Result<Option<Self>> {
        let cache_path = cache_dir.join(CACHE_FILE);
        if !cache_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&cache_path)
            .with_context(|| format!("read cache file: {cache_path}"))?;

        let cache: Self = serde_json::from_str(&content)
            .with_context(|| format!("parse cache file: {cache_path}"))?;

        // Check version compatibility
        if cache.version != CACHE_VERSION {
            return Ok(None); // Incompatible version, treat as cache miss
        }

        Ok(Some(cache))
    }

    /// Save cache to disk.
    pub fn save(&self, cache_dir: &Utf8Path) -> Result<()> {
        fs::create_dir_all(cache_dir)
            .with_context(|| format!("create cache directory: {cache_dir}"))?;

        let cache_path = cache_dir.join(CACHE_FILE);
        let content = serde_json::to_string_pretty(self).context("serialize cache")?;

        // Write atomically via temp file
        let tmp_path = cache_dir.join(".repo-state.json.tmp");
        fs::write(&tmp_path, &content)
            .with_context(|| format!("write cache temp file: {tmp_path}"))?;
        fs::rename(&tmp_path, &cache_path)
            .with_context(|| format!("rename cache file: {tmp_path} -> {cache_path}"))?;

        Ok(())
    }

    /// Delete the cache file.
    pub fn delete(cache_dir: &Utf8Path) -> Result<()> {
        let cache_path = cache_dir.join(CACHE_FILE);
        cache_path
            .exists()
            .then(|| {
                fs::remove_file(&cache_path)
                    .with_context(|| format!("delete cache file: {cache_path}"))
            })
            .transpose()?;
        Ok(())
    }
}

/// Get file metadata for cache validation.
pub fn get_file_meta(root: &Utf8Path, path: &Utf8Path) -> Result<Option<FileMeta>> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };

    if !abs_path.exists() {
        return Ok(None);
    }

    let metadata = fs::metadata(&abs_path).with_context(|| format!("get metadata: {abs_path}"))?;

    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let content = fs::read(&abs_path).with_context(|| format!("read file for hash: {abs_path}"))?;

    let hash = compute_hash(&content);

    let rel_path = if path.is_absolute() {
        path.strip_prefix(root)
            .map(|p| p.to_string())
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    };

    Ok(Some(FileMeta {
        path: rel_path,
        mtime,
        content_hash: hash,
    }))
}

/// Check if a cached file is still valid (unchanged).
///
/// This function uses a two-tier validation approach:
/// 1. First checks mtime - if different, the file may have changed
/// 2. Then checks content hash - definitive check for actual changes
///
/// The content hash is always checked as the authoritative source of truth
/// because mtime can have coarse granularity on some file systems.
pub fn is_cache_valid(root: &Utf8Path, cached: &FileMeta) -> bool {
    match get_file_meta(root, Utf8Path::new(&cached.path)) {
        Ok(Some(current)) => {
            // Content hash is the authoritative check
            current.content_hash == cached.content_hash
        }
        _ => false,
    }
}

/// Compute SHA-256 hash of content.
fn compute_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// Configuration for caching behavior.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled.
    pub enabled: bool,
    /// Cache directory path (relative to repo root or absolute).
    pub cache_dir: Utf8PathBuf,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_dir: Utf8PathBuf::from(DEFAULT_CACHE_DIR),
        }
    }
}

impl CacheConfig {
    /// Create a config with caching disabled.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Get the absolute cache directory path.
    pub fn cache_dir_abs(&self, root: &Utf8Path) -> Utf8PathBuf {
        if self.cache_dir.is_absolute() {
            self.cache_dir.clone()
        } else {
            root.join(&self.cache_dir)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let mut cache = RepoStateCache::new();
        cache.cargo_root.workspace_msrv = Some("1.70.0".to_string());
        cache.cargo_root.is_workspace = true;
        cache.lockfile_exists = true;

        // Save
        cache.save(&cache_dir).unwrap();

        // Load
        let loaded = RepoStateCache::load(&cache_dir).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.cargo_root.workspace_msrv, Some("1.70.0".to_string()));
        assert!(loaded.cargo_root.is_workspace);
        assert!(loaded.lockfile_exists);
    }

    #[test]
    fn test_cache_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let loaded = RepoStateCache::load(&cache_dir).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_cache_load_incompatible_version_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let mut cache = RepoStateCache::new();
        cache.version = CACHE_VERSION + 1;
        cache.save(&cache_dir).unwrap();

        let loaded = RepoStateCache::load(&cache_dir).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_cache_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let cache = RepoStateCache::new();
        cache.save(&cache_dir).unwrap();

        let cache_path = cache_dir.join(CACHE_FILE);
        assert!(cache_path.exists());

        RepoStateCache::delete(&cache_dir).unwrap();
        assert!(!cache_path.exists());
    }

    #[test]
    fn test_file_meta() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        // Create a test file
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let meta = get_file_meta(&root, Utf8Path::new("test.txt")).unwrap();
        assert!(meta.is_some());
        let meta = meta.unwrap();

        assert_eq!(meta.path, "test.txt");
        assert!(!meta.content_hash.is_empty());
        assert_eq!(meta.content_hash.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_file_meta_missing_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let meta = get_file_meta(&root, Utf8Path::new("missing.txt")).unwrap();
        assert!(meta.is_none());
    }

    #[test]
    fn test_file_meta_absolute_path_is_relativized() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let file_path = root.join("abs.txt");
        fs::write(&file_path, "data").unwrap();

        let meta = get_file_meta(&root, &file_path).unwrap().unwrap();
        assert_eq!(meta.path, "abs.txt");
    }

    #[test]
    fn test_cache_validation() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        // Create a test file
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let meta = get_file_meta(&root, Utf8Path::new("test.txt"))
            .unwrap()
            .unwrap();

        // Should be valid
        assert!(is_cache_valid(&root, &meta));

        // Modify file
        fs::write(&file_path, "hello world modified").unwrap();

        // Should be invalid (content changed)
        assert!(!is_cache_valid(&root, &meta));
    }

    #[test]
    fn test_cache_validation_missing_file_returns_false() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let cached = FileMeta {
            path: "missing.txt".to_string(),
            mtime: 0,
            content_hash: "deadbeef".to_string(),
        };

        assert!(!is_cache_valid(&root, &cached));
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.cache_dir.as_str(), DEFAULT_CACHE_DIR);
    }

    #[test]
    fn test_cache_config_disabled() {
        let config = CacheConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_cache_dir_abs() {
        let config = CacheConfig::default();
        let root = Utf8Path::new("/repo");
        let abs = config.cache_dir_abs(root);
        // Path separators may differ by platform
        assert!(abs.as_str().ends_with(".builddiag-cache"));
        assert!(abs.as_str().contains("repo"));
    }

    #[test]
    fn test_cache_dir_abs_with_absolute_override() {
        let temp_dir = TempDir::new().unwrap();
        let abs_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let config = CacheConfig {
            enabled: true,
            cache_dir: abs_dir.clone(),
        };

        let root = Utf8Path::new("/repo");
        let resolved = config.cache_dir_abs(root);
        assert_eq!(resolved, abs_dir);
    }

    // =========================================================================
    // Error-path coverage tests
    // =========================================================================

    #[test]
    fn test_cache_load_malformed_json_returns_err() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // Write a malformed JSON cache file.
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = cache_dir.join(CACHE_FILE);
        fs::write(&cache_path, "{not valid json").unwrap();

        let result = RepoStateCache::load(&cache_dir);
        // Implementation returns Err with context for parse failures.
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("parse cache file"));
    }

    #[test]
    fn test_cache_load_unreadable_file_returns_err() {
        // Cache file path is a directory (not a file) so read_to_string errors.
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // Create a directory at the cache file's expected path so the `exists()`
        // check passes but `read_to_string` fails.
        let cache_path = cache_dir.join(CACHE_FILE);
        fs::create_dir_all(&cache_path).unwrap();

        let result = RepoStateCache::load(&cache_dir);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("read cache file"));
    }

    #[test]
    fn test_cache_save_creates_missing_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .unwrap()
            .join("nested/sub/cache-dir");
        // Parent directories don't yet exist.
        assert!(!cache_dir.exists());

        let cache = RepoStateCache::new();
        cache.save(&cache_dir).unwrap();

        assert!(cache_dir.join(CACHE_FILE).exists());
    }

    #[test]
    fn test_cache_save_fails_when_dir_path_is_a_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // Create a file at the location we'll attempt to use as cache dir.
        let dir_as_file = root.join("not-a-dir");
        fs::write(&dir_as_file, "I am a file").unwrap();

        let cache = RepoStateCache::new();
        let result = cache.save(&dir_as_file);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("create cache directory"));
    }

    #[test]
    fn test_cache_delete_no_op_when_missing() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        // No cache file exists.
        let cache_path = cache_dir.join(CACHE_FILE);
        assert!(!cache_path.exists());

        // Should succeed without error.
        RepoStateCache::delete(&cache_dir).unwrap();
        assert!(!cache_path.exists());
    }

    #[test]
    fn test_cache_invalidates_after_file_modification() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let file_path = temp_dir.path().join("tracked.txt");
        fs::write(&file_path, "original").unwrap();

        let meta = get_file_meta(&root, Utf8Path::new("tracked.txt"))
            .unwrap()
            .unwrap();
        assert!(is_cache_valid(&root, &meta));

        // Modify content - hash should differ.
        fs::write(&file_path, "modified content here").unwrap();
        assert!(!is_cache_valid(&root, &meta));
    }

    #[test]
    fn test_cache_disabled_config_flags() {
        let config = CacheConfig::disabled();
        assert!(!config.enabled);
        // Cache dir defaults still present.
        assert_eq!(config.cache_dir.as_str(), DEFAULT_CACHE_DIR);
    }

    #[test]
    fn test_save_then_load_roundtrip_preserves_members() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let mut cache = RepoStateCache::new();
        cache.members.insert(
            "crates/foo/Cargo.toml".to_string(),
            CachedMember {
                meta: FileMeta {
                    path: "crates/foo/Cargo.toml".to_string(),
                    mtime: 0,
                    content_hash: "deadbeef".to_string(),
                },
                name: "foo".to_string(),
                rust_version: Some("1.70.0".to_string()),
                rust_version_workspace: false,
                edition: Some("2021".to_string()),
                edition_workspace: false,
                has_binary_target: false,
            },
        );
        cache.save(&cache_dir).unwrap();

        let loaded = RepoStateCache::load(&cache_dir).unwrap().unwrap();
        assert_eq!(loaded.members.len(), 1);
        let m = loaded.members.get("crates/foo/Cargo.toml").unwrap();
        assert_eq!(m.name, "foo");
        assert_eq!(m.rust_version.as_deref(), Some("1.70.0"));
        assert_eq!(m.edition.as_deref(), Some("2021"));
    }

    #[test]
    fn test_get_file_meta_with_absolute_path_outside_root_keeps_path() {
        // When absolute path is not under root, strip_prefix fails and the
        // implementation falls back to the original path string.
        let temp_dir_a = TempDir::new().unwrap();
        let temp_dir_b = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir_a.path().to_path_buf()).unwrap();
        let abs_path = temp_dir_b.path().join("outside.txt");
        fs::write(&abs_path, "data").unwrap();
        let abs_utf8 = Utf8PathBuf::from_path_buf(abs_path).unwrap();

        let meta = get_file_meta(&root, &abs_utf8).unwrap().unwrap();
        // Path should be the original absolute path (strip_prefix failed).
        assert_eq!(meta.path, abs_utf8.as_str());
    }
}
