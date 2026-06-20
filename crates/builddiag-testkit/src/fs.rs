//! File-system helpers for tests.

use std::fs;
use std::path::Path;

/// Write a file under a directory, creating parent directories when needed.
pub fn write_file(dir: &Path, rel: impl AsRef<Path>, contents: &str) {
    let path = dir.join(rel.as_ref());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}
