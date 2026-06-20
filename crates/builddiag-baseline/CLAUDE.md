# builddiag-baseline

Baseline storage and regression filtering support for builddiag.

## Purpose

Provides deterministic baseline operations:
- Build a baseline snapshot from a `builddiag.report.v1` report
- Merge new findings into an existing baseline
- Filter a report to only findings not present in baseline
- Filter findings via inline `builddiag:ignore` comments in `Cargo.toml`
- Persist baseline files atomically

## Conventions

- Baseline schema id: `builddiag.baseline.v1`
- Fingerprints are derived from `builddiag_domain::compute_fingerprint`
- Entries are always sorted and deduplicated by fingerprint
- Filtering only affects findings-based verdicts (tool/runtime errors are preserved)
