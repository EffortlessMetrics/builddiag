# builddiag-checks

Check implementation layer containing all validation logic.

## Purpose

Implements all builddiag validation checks:
- MSRV validation
- Toolchain pinning
- Checksum verification
- Workspace configuration
- Dependency hygiene (via depguard)

## Builtin Checks (15 total)

### Rust Checks
- `rust.msrv_defined` - MSRV is explicitly defined
- `rust.msrv_consistent` - All members have consistent MSRV
- `rust.toolchain_pinning` - Toolchain pinned to specific version
- `rust.toolchain_msrv_relation` - Toolchain ≥ MSRV (or equals, per policy)

### Tools Checks
- `tools.checksums_present` - Checksums file exists
- `tools.checksums_complete` - All declared tools have checksums
- `tools.checksums_valid` - Checksum format is valid SHA256
- `tools.checksums_verified` - Downloaded tools match checksums

### Workspace Checks
- `workspace.resolver_v2` - Uses resolver v2
- `workspace.edition_consistent` - Consistent edition across members
- `workspace.member_ordering` - Members alphabetically sorted

### Dependency Checks (via depguard)
- `deps.wildcard_version` - No `foo = "*"` specifications
- `deps.path_missing_version` - Path deps have version for publishing
- `deps.missing_workspace_inheritance` - Suggests workspace inheritance
- `deps.lockfile_present` - Cargo.lock exists for binary crates

## Key Types

- `CheckDocumentation` - Metadata (id, name, description, help, codes)
- `CheckDef` - Definition (id, default_severity, default_triggers)

## Key Functions

- `run_selected_checks(repo, config)` - Main execution, runs enabled checks
- `explain_check(id_or_code)` - Lookup documentation by ID or finding code

## Conventions

- Each check function returns `Result<CheckReport>`
- Checks respect profile severity and user overrides
- Diff-aware mode skips checks when no matching files changed
- All checks must have documentation (enforced by contract tests)

## Dependencies

- `builddiag-types`, `builddiag-domain`, `builddiag-repo`, `depguard`
- External: anyhow, globset, sha2, hex, camino

## Testing

- Unit tests per check
- Property tests in `tests/check_properties.rs`
- Contract tests ensuring documentation coverage
