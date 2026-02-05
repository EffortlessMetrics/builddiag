# builddiag — Implementation Plan

This is sequenced to keep boundaries sharp and avoid scope creep.

> See also: Each crate has its own `CLAUDE.md` with implementation details.

## Phase 0 — Contract Freeze + Docs (Current)

- [x] Commit the core docs:
  - docs/requirements.md
  - docs/architecture.md
  - docs/checks.md
  - docs/config.md
  - docs/design.md
  - docs/testing.md
  - docs/implementation.md
- [x] Ensure the receipt schema is generated and versioned (`builddiag.report.v1`)
- [x] Confirm canonical artifact defaults: `artifacts/builddiag/report.json` and `comment.md`
- [x] Implement profile system (oss, team, strict)
- [x] Implement `effective_check_config()` for centralized config resolution

## Phase 1 — Conformance Hardening

- [x] Schema validation test for emitted receipts (JSON schema generated via `xtask schema`)
- [x] Deterministic ordering tests (byte-stable JSON + MD)
- [x] Explain registry completeness tests:
  - Every emitted (check_id, code) must be documented
  - Contract tests in `builddiag-checks/src/lib.rs`
- [x] Add/confirm `xtask schema` and CI wiring
- [x] (Windows) Ensure `xtask ci` uses a separate target dir to avoid self-rebuild locking

## Phase 2 — Check Set Polish

- [x] Confirm boundaries:
  - Remove/avoid any machine-truth verification (local binary hashing) by default
  - `tools.checksums_verify_local` is opt-in via policy
- [x] Tighten MSRV/toolchain/workspace checks for clarity and remediation text
- [x] Ensure `oss` profile is safe:
  - Missing convention files => skip, not fail
  - Malformed present files => warn/fail as real signal
- [x] Add depguard integration for dependency hygiene checks:
  - `deps.wildcard_version`
  - `deps.path_missing_version`
  - `deps.workspace_inheritance`
- [x] Add workspace checks:
  - `workspace.edition_consistent`
  - `workspace.member_ordering`
- [x] Add lockfile check:
  - `deps.lockfile_present`

## Phase 3 — UX and Adoption

- [x] Ensure CLI ergonomics are consistent:
  - `check`, `md`, `github-annotations`, `explain`, `list-checks`
  - Stable exit codes (0 ok, 1 tool error, 2 policy fail)
- [x] Provide copy/paste snippets in README:
  - Local usage
  - CI usage (artifact paths, annotation flags)
- [x] Add `--profile` CLI flag for easy profile switching
- [x] Add `--config` CLI flag for config file override

## Phase 4 — Distribution

- [x] Prebuilt binaries for Linux/macOS/Windows
- [x] Crates.io publish posture:
  - Publish CLI crate and dependency crates
  - Document internal crates as implementation detail
- [x] GitHub releases with changelogs

## Phase 5 — Ecosystem Integration Alignment

- [x] Align builddiag's receipt envelope semantics with cockpit contract:
  - Minimal required fields
  - One extension point (`data`)
  - Avoid multiple near-envelopes
- [x] Document integration patterns:
  - CI workflows (GitHub Actions)
  - Local development hooks
  - Pre-commit integration

## Future Considerations

### Potential New Checks

- `workspace.publish_ready` — Verify publishable crates have required metadata
- `rust.edition_deprecations` — Warn about deprecated edition features
- `deps.duplicate_versions` — Detect dependency version conflicts
- `deps.security_advisory` — Check against RustSec advisory database

### Integration Points

- **GitHub Actions**: Reusable workflow for builddiag
- **Pre-commit hooks**: builddiag as a pre-commit hook
- **IDE integration**: Language server diagnostics from builddiag

### Performance Optimizations

- Parallel check execution
- Incremental checking (cache repo state)
- Lazy loading of member manifests
