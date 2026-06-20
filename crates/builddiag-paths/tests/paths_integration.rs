//! Integration tests for builddiag-paths.

use builddiag_paths::{join_normalized, to_repo_relative};
use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

#[test]
fn to_repo_relative_handles_nested_paths() {
    let temp_dir = TempDir::new().unwrap();
    let root_std = temp_dir.path().join("workspace");
    std::fs::create_dir_all(root_std.join("crates/a")).unwrap();

    let file_std = root_std.join("crates/a/Cargo.toml");
    std::fs::write(&file_std, "").unwrap();

    let root = Utf8Path::from_path(&root_std).unwrap();
    let file = Utf8Path::from_path(&file_std).unwrap();
    let relative = to_repo_relative(root, file);

    assert_eq!(relative, "crates/a/Cargo.toml");
    assert!(!relative.contains('\\'));
}

#[test]
fn join_normalized_produces_forward_slashes() {
    let temp_dir = TempDir::new().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();
    let joined = join_normalized(&root, "crates\\demo\\Cargo.toml");

    let joined_str = joined.as_str();
    assert!(joined_str.contains("crates/demo/Cargo.toml"));
    assert!(!joined_str.contains('\\'));
}
