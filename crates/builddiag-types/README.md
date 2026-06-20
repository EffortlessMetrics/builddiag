# builddiag-types

Shared schemas and data types for the builddiag workspace.

## What this crate provides

- Native report models for `builddiag.report.v1`
- Sensor envelope models for `sensor.report.v1`
- Config/profile/policy types used by checks and CLI
- `Substrate` and `ManifestInfo` for pre-computed repo input

## Design constraints

- Serializable with `serde` and schema-ready with `schemars`
- Deterministic ordering (`BTreeMap`/`BTreeSet`) for stable JSON output
- No I/O and no dependencies on other builddiag crates

## Example

```rust
use builddiag_types::Config;

let cfg = Config::default();
assert_eq!(cfg.defaults.out_dir, "artifacts/builddiag");
```
