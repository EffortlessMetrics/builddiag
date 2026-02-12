# builddiag-domain

Pure domain logic layer with no I/O operations.

## Purpose

Implements core business logic:
- Rust version parsing and normalization
- Check result aggregation
- Verdict determination
- Exit code mapping
- Finding sorting for deterministic output

## Key Functions

### Version Handling
- `parse_rust_version(s)` - Normalizes version strings (e.g., "1.75" → "1.75.0")

### Aggregation
- `check_status_from_findings(findings)` - Derives CheckStatus from finding severities
- `summarize(check_reports)` - Aggregates CheckReports into Summary with counts
- `determine_verdict(statuses)` - Computes overall verdict from check statuses

### Output
- `exit_code_for(verdict, fail_on)` - Maps verdict + policy to exit code (0 or 2)
- `sort_findings_canonical(findings)` - Stable sort: severity desc → check_id → path → line → code → message

### Documentation
- `explain` module - Provides detailed explanations for checks and finding codes

## Conventions

- All functions are pure (no side effects, no I/O)
- Functions take references where possible
- Return `anyhow::Result` for fallible operations
- Canonical sorting ensures byte-stable output across runs

## Dependencies

- `builddiag-types` only
- External: anyhow, semver

## Testing

- Unit tests inline in `src/lib.rs`
- Property tests in `tests/domain_properties.rs` using proptest
- Focus on invariants like sorting stability and version normalization
