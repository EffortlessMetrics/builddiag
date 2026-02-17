# builddiag-domain

Pure domain logic for builddiag with zero filesystem or process I/O.

## What this crate provides

- Rust version parsing and normalization (`parse_rust_version`)
- Check/report aggregation (`summarize`, `determine_verdict`)
- Exit code policy mapping (`exit_code_for`)
- Canonical finding sorting and stable fingerprints
- Sensor-verdict construction helpers

## Design constraints

- Deterministic pure functions
- Depends only on `builddiag-types` plus utility crates
- Suitable for property testing and fuzzing

## Example

```rust
use builddiag_domain::parse_rust_version;

let v = parse_rust_version("1.75").unwrap();
assert_eq!(v.to_string(), "1.75.0");
```
