# builddiag-paths

Cross-platform path normalization helpers for builddiag.

## What this crate provides

- Forward-slash normalization for deterministic paths
- Repo-relative path conversion
- Normalized path joining for repo-relative inputs

## Key APIs

- `normalize_slashes(...)`
- `to_repo_relative(...)`
- `join_normalized(...)`
