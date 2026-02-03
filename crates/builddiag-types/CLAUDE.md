# builddiag-types

Foundation layer providing all shared data structures for the builddiag ecosystem.

## Purpose

This crate defines serializable types for:
- JSON report generation and parsing
- Configuration schema
- Profile system for check severity presets
- Finding and location structures

## Key Types

### Report & Output
- `Report` - Main output with schema v1 format, check findings, and metadata
- `Verdict` - Overall result enum: `Pass`, `Warn`, `Fail`
- `Finding` - Individual validation issue with severity, code, message, location
- `CheckReport` - Results from single check execution
- `Summary` - Aggregated counts by severity and check

### Configuration
- `Config` - Root configuration schema for check behavior
- `Profile` - Enum: `Oss`, `Team`, `Strict` - preset severity mappings
- `CheckConfig` - Per-check configuration (severity, triggers, enabled)
- `effective_check_config()` - Resolves profile defaults with user overrides

### Policy Types
- `MsrvPolicy`, `ToolchainPolicy`, `ChecksumsPolicy`
- `EditionPolicy`, `MemberOrderingPolicy`, `LockfilePolicy`

### Metadata
- `HostInfo`, `GitInfo`, `RunInfo`, `ToolInfo`
- `Location` - File position (path, line, column)

### Status Enums
- `Severity` - `Info`, `Warn`, `Error`
- `CheckStatus` - `Pass`, `Warn`, `Fail`, `Skip`

## Conventions

- All public types derive `Serialize`, `Deserialize`, `JsonSchema`
- Use `BTreeMap`/`BTreeSet` for deterministic JSON ordering
- Types are read-only data structures (no methods with side effects)
- Doc comments on all public items

## Dependencies

- No dependencies on other builddiag crates (foundation layer)
- External: serde, serde_json, chrono, schemars

## Testing

Unit tests inline in `src/lib.rs` covering:
- Serialization round-trips
- Profile severity mappings
- Config merging logic
