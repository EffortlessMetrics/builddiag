//! Path normalization utilities for builddiag.
//!
//! This crate provides deterministic path handling helpers for producing
//! repo-relative, forward-slash paths across all platforms.

use camino::{Utf8Path, Utf8PathBuf};

/// Normalizes a path to use forward slashes on all platforms.
///
/// This ensures consistent path representation regardless of the operating system.
///
/// # Examples
///
/// ```
/// use builddiag_paths::normalize_slashes;
///
/// assert_eq!(normalize_slashes("foo\\bar\\baz"), "foo/bar/baz");
/// assert_eq!(normalize_slashes("foo/bar/baz"), "foo/bar/baz");
/// ```
pub fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

/// Converts an absolute path to a repo-relative path with forward slashes.
///
/// If the path is not under the repo root, returns the original path normalized.
///
/// # Examples
///
/// ```
/// use builddiag_paths::to_repo_relative;
/// use camino::Utf8Path;
///
/// let repo_root = Utf8Path::new("/home/user/project");
/// let abs_path = Utf8Path::new("/home/user/project/crates/foo/Cargo.toml");
/// assert_eq!(to_repo_relative(repo_root, abs_path), "crates/foo/Cargo.toml");
/// ```
pub fn to_repo_relative(repo_root: &Utf8Path, abs_path: &Utf8Path) -> String {
    let repo_str = normalize_slashes(repo_root.as_str());
    let abs_str = normalize_slashes(abs_path.as_str());

    // Handle both with and without trailing slash
    let repo_prefix = if repo_str.ends_with('/') {
        repo_str.clone()
    } else {
        format!("{}/", repo_str)
    };

    if abs_str.starts_with(&repo_prefix) {
        abs_str[repo_prefix.len()..].to_string()
    } else if abs_str == repo_str.trim_end_matches('/') {
        ".".to_string()
    } else {
        abs_str
    }
}

/// Joins a repo-relative path to a root, returning a normalized absolute path.
pub fn join_normalized(root: &Utf8Path, relative: &str) -> Utf8PathBuf {
    let joined = root.join(relative);
    Utf8PathBuf::from(normalize_slashes(joined.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Path normalization utilities
    // =========================================================================

    #[test]
    fn normalize_slashes_converts_backslashes() {
        assert_eq!(normalize_slashes("foo\\bar\\baz"), "foo/bar/baz");
        assert_eq!(normalize_slashes("a\\b\\c\\d"), "a/b/c/d");
    }

    #[test]
    fn normalize_slashes_preserves_forward_slashes() {
        assert_eq!(normalize_slashes("foo/bar/baz"), "foo/bar/baz");
    }

    #[test]
    fn normalize_slashes_handles_mixed_slashes() {
        assert_eq!(normalize_slashes("foo\\bar/baz\\qux"), "foo/bar/baz/qux");
    }

    #[test]
    fn normalize_slashes_handles_empty_string() {
        assert_eq!(normalize_slashes(""), "");
    }

    #[test]
    fn to_repo_relative_removes_prefix() {
        let root = Utf8Path::new("/home/user/project");
        let abs = Utf8Path::new("/home/user/project/crates/foo/Cargo.toml");
        assert_eq!(to_repo_relative(root, abs), "crates/foo/Cargo.toml");
    }

    #[test]
    fn to_repo_relative_handles_root_with_trailing_slash() {
        let root = Utf8Path::new("/home/user/project/");
        let abs = Utf8Path::new("/home/user/project/src/lib.rs");
        assert_eq!(to_repo_relative(root, abs), "src/lib.rs");
    }

    #[test]
    fn to_repo_relative_returns_dot_for_root() {
        let root = Utf8Path::new("/home/user/project");
        let abs = Utf8Path::new("/home/user/project");
        assert_eq!(to_repo_relative(root, abs), ".");
    }

    #[test]
    fn to_repo_relative_returns_normalized_for_non_child() {
        let root = Utf8Path::new("/home/user/project");
        let abs = Utf8Path::new("/home/other/file.txt");
        // When not under root, returns normalized path
        assert_eq!(to_repo_relative(root, abs), "/home/other/file.txt");
    }

    #[test]
    fn join_normalized_converts_backslashes() {
        let temp_dir = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
        let joined = join_normalized(&root, "crates\\demo\\Cargo.toml");

        let joined_str = joined.as_str();
        assert!(joined_str.contains("crates/demo/Cargo.toml"));
        assert!(!joined_str.contains('\\'));
    }
}
