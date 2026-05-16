# builddiag-repo

Repository discovery and repo-state loading for builddiag.

## What this crate provides

- Workspace/member discovery from Cargo metadata
- Manifest parsing for workspace/package MSRV, edition, publish metadata
- Tooling file loading (`rust-toolchain`, tools manifest, checksums)
- Cross-platform path normalization to repo-relative forward-slash paths
- Optional repo-state caching (`cache` feature, enabled by default)

## Key APIs

- `load_repo_state(...)`
- `load_repo_state_cached(...)` (`cache` feature)
- `discover_workspace(...)`
- `repo_state_from_substrate(...)`

## Design constraints

- Deterministic ordering of members and parsed structures
- Works for single-crate and multi-crate workspaces
- Uses `camino::Utf8Path` throughout
