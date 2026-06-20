# builddiag-fix

Auto-fix planning and apply logic for `builddiag fix`.

## Purpose

Provide deterministic, unambiguous auto-remediation for a focused set of
build-contract issues.

## Supported Fixes

- Add `workspace.package.rust-version` when derivable from numeric toolchain
  or a single consistent member MSRV.
- Set `workspace.resolver = "2"` for workspace roots.
- Add missing checksums entries based on `scripts/tools.toml` declarations.

## Public API

- `plan_fixes(root, config)` - Build proposals + warnings without writing.
- `apply_fixes(root, config, options, confirm)` - Apply (or dry-run) proposals.

## Constraints

- Skip ambiguous fixes and emit warnings instead of guessing.
- Keep ordering deterministic for stable dry-run output.
- No CLI concerns in this crate; argument parsing/prompting belongs in
  `builddiag-cli`.
