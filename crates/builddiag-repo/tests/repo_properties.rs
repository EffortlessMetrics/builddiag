//! Property tests for builddiag-repo workspace discovery.
//!
//! These tests verify key invariants of workspace discovery:
//! - Determinism: same input always produces same output
//! - No duplicates: each manifest appears exactly once
//! - Glob expansion correctness: patterns match expected directories

use builddiag_repo::{discover_workspace, normalize_slashes, to_repo_relative};
use camino::{Utf8Path, Utf8PathBuf};
use proptest::prelude::*;
use std::collections::HashSet;
use tempfile::TempDir;

// ============================================================================
// Path Normalization Properties
// ============================================================================

proptest! {
    /// Property: normalize_slashes is idempotent
    #[test]
    fn prop_normalize_slashes_idempotent(path in "[a-z/\\\\]+") {
        let once = normalize_slashes(&path);
        let twice = normalize_slashes(&once);
        prop_assert_eq!(once, twice);
    }

    /// Property: normalize_slashes never contains backslashes
    #[test]
    fn prop_normalize_slashes_no_backslashes(path in "[a-z/\\\\]+") {
        let normalized = normalize_slashes(&path);
        prop_assert!(!normalized.contains('\\'));
    }

    /// Property: normalize_slashes preserves path segments
    #[test]
    fn prop_normalize_slashes_preserves_segments(segments in prop::collection::vec("[a-z]+", 1..5)) {
        // Join with backslashes
        let backslash_path = segments.join("\\");
        // Join with forward slashes
        let forward_path = segments.join("/");

        let normalized = normalize_slashes(&backslash_path);
        prop_assert_eq!(normalized, forward_path);
    }
}

// ============================================================================
// Workspace Discovery Properties
// ============================================================================

/// Helper to create a minimal Cargo.toml for a package.
fn make_package_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#
    )
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

/// Strategy for generating valid crate names (lowercase alphanumeric with hyphens).
fn crate_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,10}[a-z0-9]?".prop_filter("non-empty and valid", |s| {
        !s.is_empty() && !s.starts_with('-') && !s.ends_with('-') && !is_windows_reserved(s)
    })
}

/// Returns `true` if the name is a Windows reserved device name.
///
/// These names (CON, PRN, AUX, NUL, COM1-9, LPT1-9) cannot be used as
/// file or directory names on Windows and will cause I/O failures.
fn is_windows_reserved(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5"
            | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5"
            | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

/// Strategy for generating a set of unique crate names.
fn unique_crate_names(count: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::hash_set(crate_name_strategy(), 1..=count)
        .prop_map(|set| set.into_iter().collect())
}

proptest! {
    /// Property: Discovery is deterministic - running twice produces identical results
    #[test]
    fn prop_discovery_deterministic(crate_names in unique_crate_names(5)) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace with glob pattern
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create crates
        for name in &crate_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();

        // Run discovery twice
        let model1 = discover_workspace(&path).unwrap();
        let model2 = discover_workspace(&path).unwrap();

        // Should have identical results
        let paths1: Vec<&str> = model1.member_manifests.keys().map(|s| s.as_str()).collect();
        let paths2: Vec<&str> = model2.member_manifests.keys().map(|s| s.as_str()).collect();

        prop_assert_eq!(paths1, paths2);
    }

    /// Property: No duplicate manifests in discovery results
    #[test]
    fn prop_no_duplicate_manifests(crate_names in unique_crate_names(5)) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create crates
        for name in &crate_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        // Collect paths into a HashSet to check for duplicates
        let paths: Vec<&str> = model.member_manifests.keys().map(|s| s.as_str()).collect();
        let unique_paths: HashSet<&str> = paths.iter().cloned().collect();

        // Number of paths should equal number of unique paths (no duplicates)
        prop_assert_eq!(paths.len(), unique_paths.len());
    }

    /// Property: Member count matches discovered manifests
    #[test]
    fn prop_member_count_matches(crate_names in unique_crate_names(5)) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create crates
        for name in &crate_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        // member_count() should match the actual number of manifests
        prop_assert_eq!(model.member_count(), model.member_manifests.len());
        // has_members() should be consistent
        prop_assert_eq!(model.has_members(), model.member_count() > 0);
    }

    /// Property: Results are always sorted alphabetically
    #[test]
    fn prop_results_sorted(crate_names in unique_crate_names(5)) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create crates
        for name in &crate_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        let paths: Vec<&str> = model.member_manifests.keys().map(|s| s.as_str()).collect();
        let mut sorted_paths = paths.clone();
        sorted_paths.sort();

        prop_assert_eq!(paths, sorted_paths, "Results should be sorted alphabetically");
    }

    /// Property: Single crate repo always has exactly one member
    #[test]
    fn prop_single_crate_one_member(name in crate_name_strategy()) {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("Cargo.toml");
        std::fs::write(&manifest_path, make_package_toml(&name)).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        prop_assert_eq!(model.member_count(), 1);
        prop_assert!(!model.is_virtual);
        prop_assert!(model.member_manifests.contains_key("Cargo.toml"));
    }

    /// Property: All discovered paths use forward slashes
    #[test]
    fn prop_paths_use_forward_slashes(crate_names in unique_crate_names(3)) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, make_workspace_toml(&["crates/*"])).unwrap();

        // Create crates
        for name in &crate_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        for path in model.member_manifests.keys() {
            prop_assert!(!path.contains('\\'), "Path should not contain backslashes: {}", path);
        }
    }
}

// ============================================================================
// Glob Expansion Properties
// ============================================================================

proptest! {
    /// Property: Exclude patterns actually exclude matching paths
    #[test]
    fn prop_exclude_actually_excludes(
        included_names in unique_crate_names(2),
        excluded_name in crate_name_strategy()
    ) {
        // Skip if excluded_name is in included_names
        prop_assume!(!included_names.contains(&excluded_name));

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create workspace with exclude
        let manifest_content = format!(
            r#"[workspace]
resolver = "2"
members = ["crates/*"]
exclude = ["crates/{}"]
"#,
            excluded_name
        );
        let manifest_path = root.join("Cargo.toml");
        std::fs::write(&manifest_path, &manifest_content).unwrap();

        // Create included crates
        for name in &included_names {
            std::fs::create_dir_all(root.join(format!("crates/{}", name))).unwrap();
            std::fs::write(
                root.join(format!("crates/{}/Cargo.toml", name)),
                make_package_toml(name),
            ).unwrap();
        }

        // Create excluded crate
        std::fs::create_dir_all(root.join(format!("crates/{}", excluded_name))).unwrap();
        std::fs::write(
            root.join(format!("crates/{}/Cargo.toml", &excluded_name)),
            make_package_toml(&excluded_name),
        ).unwrap();

        let path = Utf8PathBuf::from_path_buf(manifest_path).unwrap();
        let model = discover_workspace(&path).unwrap();

        // The excluded crate should not be in results
        let excluded_path = format!("crates/{}/Cargo.toml", excluded_name);
        prop_assert!(
            !model.member_manifests.contains_key(&excluded_path),
            "Excluded path {} should not be in results",
            excluded_path
        );

        // All included crates should be present
        for name in &included_names {
            let included_path = format!("crates/{}/Cargo.toml", name);
            prop_assert!(
                model.member_manifests.contains_key(&included_path),
                "Included path {} should be in results",
                included_path
            );
        }
    }
}

// ============================================================================
// to_repo_relative Properties
// ============================================================================

proptest! {
    /// Property: to_repo_relative result never starts with the root path
    #[test]
    fn prop_repo_relative_removes_root(
        root_segments in prop::collection::vec("[a-z]+", 2..4),
        file_segments in prop::collection::vec("[a-z]+", 1..3)
    ) {
        let root_path = format!("/{}", root_segments.join("/"));
        let file_path = format!("{}/{}", root_path, file_segments.join("/"));

        let root = Utf8Path::new(&root_path);
        let file = Utf8Path::new(&file_path);

        let relative = to_repo_relative(root, file);

        // Result should not start with a slash (unless it's not under root)
        if file_path.starts_with(&root_path) {
            prop_assert!(!relative.starts_with('/'), "Relative path should not start with /: {}", relative);
        }
    }
}
