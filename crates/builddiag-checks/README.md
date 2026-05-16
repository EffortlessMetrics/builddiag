# builddiag-checks

Check registry and check implementations used by builddiag.

## What this crate provides

- Built-in check definitions (`BUILTIN_CHECKS`)
- Check documentation registry (`CHECK_DOCS`) used by `builddiag explain`
- Execution entry point (`run_selected_checks`)
- Check lookup by id or finding code (`explain_check`)

Checks cover Rust toolchain/MSRV, workspace policy, dependency hygiene, checksums, publish metadata, and optional security advisory scanning.

## Features

- `parallel` (default): execute checks with Rayon
- `security`: enable RustSec advisory checks

## Design constraints

- Deterministic report ordering
- Diff-aware trigger support via per-check file patterns
- Profile and override aware severity/enabled resolution
