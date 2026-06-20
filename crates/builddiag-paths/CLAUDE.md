# builddiag-paths

Path normalization helpers for builddiag.

## Purpose

Provide deterministic, cross-platform path handling for repo-relative outputs and reports.

## Key Functions

- `normalize_slashes`
- `to_repo_relative`
- `join_normalized`

## Conventions

- All outputs use forward slashes.
- Inputs are `camino::Utf8Path` wherever possible.

## Testing

- Unit tests in `src/lib.rs`
- Property tests in `tests/paths_properties.rs`
- Integration tests in `tests/paths_integration.rs`
