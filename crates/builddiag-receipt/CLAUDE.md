# CLAUDE.md

# builddiag-receipt

Receipt utilities and sensor interoperability adapters for builddiag.

## Purpose

This crate owns the transformation layer between builddiag-native reports and
Cockpit-compatible `sensor.report.v1` artifacts.

## API Surface

- `build_capabilities`
- `build_capabilities_with_substrate`
- `report_to_sensor`
- `create_error_receipt`

### Feature Flags

- `with-substrate` (default): keep explicit `substrate` capability tracking
  for substrate-driven repo-state pipelines.

## Design

- No filesystem IO.
- Pure data-shaping and status/capability mapping.
- Depends only on `builddiag-domain` and `builddiag-types` for deterministic behavior.
