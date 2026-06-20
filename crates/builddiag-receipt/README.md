# builddiag-receipt

Shared receipt and sensor interoperability primitives for builddiag.

This crate centralizes logic previously spread through `builddiag-app`:
- capability generation for receipt `run.capabilities`
- conversion from `Report` to `SensorReport`
- canonical runtime error receipt creation (`tool.runtime/runtime_error`)

The crate is intentionally framework-agnostic and does not perform filesystem IO.

Feature behavior:

- `with-substrate` (default): `build_capabilities_with_substrate` records
  `substrate` when the caller indicates substrate mode was used.
- without `with-substrate`: the `substrate` flag input is intentionally ignored,
  preserving compatibility for no-substrate builds.
