# builddiag-checks-deps

Dependency-focused checks for `builddiag`.

## Checks

- `deps.wildcard_version`
- `deps.path_missing_version`
- `deps.workspace_inheritance`
- `deps.lockfile_present`
- `deps.duplicate_versions`
- `deps.security_advisory`

This crate is intentionally small and isolated behind `builddiag-checks` feature
gating so each family of checks can evolve independently.

## Security feature

`deps.security_advisory` requires the optional `security` feature. When that feature
is disabled, the check reports `Skip` with `feature_not_available`.

