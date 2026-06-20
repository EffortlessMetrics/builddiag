#![no_main]

use arbitrary::Arbitrary;
use builddiag_paths::{join_normalized, normalize_slashes, to_repo_relative};
use camino::Utf8Path;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct PathInput {
    root: String,
    path: String,
    relative: String,
}

fuzz_target!(|input: PathInput| {
    let root = Utf8Path::new(&input.root);
    let path = Utf8Path::new(&input.path);

    let normalized = normalize_slashes(&input.path);
    assert!(!normalized.contains('\\'));

    let rel = to_repo_relative(root, path);
    assert!(!rel.contains('\\'));

    let joined = join_normalized(root, &input.relative);
    assert!(!joined.as_str().contains('\\'));
});
