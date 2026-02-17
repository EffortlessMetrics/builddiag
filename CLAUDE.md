# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Builddiag is a Rust CLI tool that validates the build contract of Rust repositories. It checks MSRV (Minimum Supported Rust Version), toolchain pinning, checksum verification, and workspace configuration. Output formats include JSON reports, Markdown summaries, and GitHub Actions annotations.

## Commands

### Build & Run
```bash
cargo build                              # Build all crates
cargo build -p builddiag-cli             # Build CLI binary only
cargo run -p builddiag -- check --root . # Run check on current directory
```

### Testing
```bash
cargo test --all                         # Run all tests (unit, property, integration)
cargo test -p builddiag-domain           # Test specific crate
cargo test test_name                     # Run single test by name
```

### Linting & Formatting
```bash
cargo fmt --all                          # Format code
cargo clippy --all-targets --all-features -- -D warnings  # Lint (must pass with no warnings)
```

### CI & Automation
```bash
cargo run -p xtask -- ci                 # Full CI check (format, lint, test, schema)
cargo run -p xtask -- conform            # Run 7 conformance checks against fixtures
cargo run -p xtask -- conform --update-golden  # Regenerate golden files
cargo run -p xtask -- schema             # Generate JSON schemas (builddiag-native only)
cargo run -p xtask -- coverage [--html]  # Code coverage
```

### Fuzz Testing (requires nightly)
```bash
cd fuzz && cargo +nightly fuzz list                                    # List targets
cd fuzz && cargo +nightly fuzz run fuzz_version -- -max_total_time=300 # Run specific
```

### Mutation Testing
```bash
cargo mutants                            # Run all mutation tests
cargo mutants -p builddiag-domain        # Test specific package
```

## Architecture

Eleven-crate Cargo workspace with layered architecture (dependencies flow downward):

```
builddiag-cli      CLI entry point, argument parsing
       ↓           ↘
builddiag-watch    Polling watch loop + debounce
builddiag-fix      Deterministic auto-fix planner/applier
       ↓
builddiag-core     builddiag-baseline
       ↓
builddiag-app      Orchestration, config loading, output writing
       ↓
builddiag-render   Markdown & GitHub annotation rendering
builddiag-checks   Check implementations (MSRV, toolchain, checksums)
       ↓
builddiag-repo     Repository state loading (Cargo.toml, rust-toolchain)
       ↓
builddiag-domain   Core logic (version parsing, summarization)
       ↓
builddiag-types    Shared types, config schema, report schema
```

### Key Types (builddiag-types)
- `Report` - Main output with check results and summary (builddiag.report.v1)
- `SensorReport` - Cockpit CI sensor envelope (sensor.report.v1)
- `SensorVerdict` - Structured verdict with status, counts, reasons, and data
- `Substrate` / `ManifestInfo` - Pre-computed repo state for library integration
- `Config` - Configuration schema for check behavior
- `Finding` - Individual validation findings with severity/location
- `CheckReport` - Results from single check execution
- `Severity` - Enum: Info, Warn, Error
- `CheckStatus` - Enum: Pass, Warn, Fail, Skip

### CLI Subcommands
- `check` - Main validation command with options for root path, config, diff-aware mode
- `md` - Render Markdown from existing JSON report
- `github-annotations` - Emit annotations from existing report

## Code Conventions

- Use `anyhow::Result` for fallible functions
- Prefer `camino::Utf8Path` over `std::path::Path`
- Derive `Serialize`, `Deserialize`, `JsonSchema` for public types
- Use `BTreeMap`/`BTreeSet` for deterministic ordering
- All public functions/types must have doc comments (`///`)

## Testing Structure

- **Unit tests**: Inline `#[cfg(test)]` modules in source files
- **Property tests**: `tests/<crate>_properties.rs` using `proptest`
- **Integration tests**: `crates/builddiag-cli/tests/` using `assert_cmd` + `predicates`
- **Conformance tests**: `xtask conform` — 7 checks (schema, determinism, survivability, layout, golden, tool-error, library-parity)
- **Fuzz tests**: `fuzz/fuzz_targets/` (6 targets: version, toml, checksums, config, report, render)

Test naming: unit tests use `test_<description>`, property tests use `prop_<property_name>`, integration tests use `<command>_<scenario>`.
