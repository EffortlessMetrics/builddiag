# builddiag-baseline

Baseline snapshot and filtering utilities for builddiag findings.

## What this crate provides

- Baseline file read/write (`builddiag.baseline.v1`)
- Snapshot creation from reports (`from_report`)
- Merge/update helpers (`merge_report`)
- Regression-only filtering (`filter_report`)
- Inline suppression filtering from `Cargo.toml` comments (`filter_report_inline_suppressions`)

## Key APIs

- `read`, `read_or_default`, `write`
- `from_report`, `merge_report`
- `filter_report`, `filter_report_inline_suppressions`

## Design constraints

- Stable fingerprinting via `builddiag-domain`
- Deterministic sorting/deduplication of baseline entries
- Tool/runtime error receipts are preserved through filtering
