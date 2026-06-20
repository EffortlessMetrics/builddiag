//! Property tests for builddiag-paths.
//!
//! These tests verify key invariants of path normalization utilities.

use builddiag_paths::{normalize_slashes, to_repo_relative};
use camino::Utf8Path;
use proptest::prelude::*;
use tempfile::TempDir;

// =============================================================================
// Proptest Configuration
// =============================================================================

/// Configure proptest to run at least 100 iterations per property.
const PROPTEST_CASES: u32 = 100;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

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

    /// Property: to_repo_relative result never includes backslashes or root prefix
    #[test]
    fn prop_repo_relative_removes_root(file_segments in prop::collection::vec("[a-z]{1,8}", 1..4)) {
        let temp_dir = TempDir::new().unwrap();
        let root_std = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&root_std).unwrap();

        let mut file_std = root_std.clone();
        for segment in &file_segments {
            file_std = file_std.join(segment);
        }
        if let Some(parent) = file_std.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_std, "").unwrap();

        let root = Utf8Path::from_path(&root_std).unwrap();
        let file = Utf8Path::from_path(&file_std).unwrap();
        let relative = to_repo_relative(root, file);

        prop_assert!(
            !relative.starts_with('/'),
            "Relative path should not start with /: {}",
            relative
        );
        prop_assert!(
            !relative.contains('\\'),
            "Relative path should not contain backslashes: {}",
            relative
        );
        prop_assert!(
            !relative.starts_with(root.as_str()),
            "Relative path should not include absolute root prefix: {}",
            relative
        );
    }
}
