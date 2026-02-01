# Testing Guide

This document describes the testing strategy, organization, and conventions for the builddiag project. For coverage-specific documentation, see [COVERAGE.md](./COVERAGE.md).

## Table of Contents

- [Quick Start](#quick-start)
- [Test Types](#test-types)
  - [Unit Tests](#unit-tests)
  - [Property-Based Tests](#property-based-tests)
  - [Integration Tests](#integration-tests)
  - [Fuzz Tests](#fuzz-tests)
  - [Mutation Tests](#mutation-tests)
- [Test Organization](#test-organization)
- [Running Tests](#running-tests)
- [Writing Tests](#writing-tests)
- [CI Integration](#ci-integration)
- [Troubleshooting](#troubleshooting)

## Quick Start

```bash
# Run all tests (unit, property, integration)
cargo test --all

# Run tests for a specific crate
cargo test -p builddiag-domain

# Run a specific test by name
cargo test test_name

# Run tests with output
cargo test --all -- --nocapture

# Full CI check (format, lint, test, schema)
cargo run -p xtask -- ci
```

## Test Types

### Unit Tests

Unit tests validate individual functions and modules in isolation. They test specific examples, edge cases, and error conditions.

**Location**: Inline `#[cfg(test)]` modules in each crate's source files.

**Framework**: Built-in Rust `#[test]` attribute with `insta` for snapshot testing.

**Running**:
```bash
# Run all unit tests
cargo test --all

# Run unit tests for a specific crate
cargo test -p builddiag-types
cargo test -p builddiag-domain
cargo test -p builddiag-repo
cargo test -p builddiag-checks
cargo test -p builddiag-render
cargo test -p builddiag-app
```

**Example**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Config defaults are correctly initialized.
    /// _Requirements: 2.1_
    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert!(config.policy.msrv.require);
    }
}
```

### Property-Based Tests

Property-based tests validate universal invariants across randomly generated inputs using the `proptest` framework. They complement unit tests by discovering edge cases through randomized testing.

**Location**: `tests/` directories in each crate (e.g., `crates/builddiag-domain/tests/domain_properties.rs`).

**Framework**: [proptest](https://docs.rs/proptest)

**Running**:
```bash
# Run all tests including property tests
cargo test --all

# Run property tests for a specific crate
cargo test -p builddiag-domain --test domain_properties
cargo test -p builddiag-types --test types_properties
cargo test -p builddiag-checks --test check_properties
cargo test -p builddiag-render --test render_properties
```

**Configuration**: Property tests run at least 100 iterations per property (configured via `ProptestConfig`).

**Properties Tested**:

| Property | Crate | Description |
|----------|-------|-------------|
| Config Round-Trip | builddiag-types | Config serializes to TOML and back without data loss |
| Report Round-Trip | builddiag-types | Report serializes to JSON and back without data loss |
| Version Normalization | builddiag-domain | Version strings parse to normalized semver format |
| Check Status Consistency | builddiag-domain | Status matches highest severity finding |
| Summary Aggregation | builddiag-domain | Summary counts equal sum of individual findings |
| Deterministic Output | builddiag-render | Same input produces identical output |
| Graceful Error Handling | builddiag-checks | Invalid input returns error without panic |
| Error Message Context | builddiag-checks | Error messages are non-empty and descriptive |

**Example**:
```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: comprehensive-test-coverage, Property 1: Config Serialization Round-Trip
    /// **Validates: Requirements 3.8**
    #[test]
    fn prop_config_roundtrip(config in arb_config()) {
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        prop_assert_eq!(config, parsed);
    }
}
```

### Integration Tests

Integration tests validate the complete CLI workflow and end-to-end behavior using `assert_cmd` and `predicates`.

**Location**: `crates/builddiag-cli/tests/`

**Framework**: [assert_cmd](https://docs.rs/assert_cmd) + [predicates](https://docs.rs/predicates) + [tempfile](https://docs.rs/tempfile)

**Running**:
```bash
# Run all integration tests
cargo test -p builddiag-cli

# Run specific integration test file
cargo test -p builddiag-cli --test cli_check
cargo test -p builddiag-cli --test cli_exit_codes
cargo test -p builddiag-cli --test cli_md
cargo test -p builddiag-cli --test cli_annotations
cargo test -p builddiag-cli --test cli_diff_aware
cargo test -p builddiag-cli --test cli_config
```

**Test Files**:

| File | Description |
|------|-------------|
| `cli_smoke.rs` | Basic smoke tests for CLI functionality |
| `cli_check.rs` | Tests for `builddiag check` command with various scenarios |
| `cli_exit_codes.rs` | Tests for exit code behavior (0, 2, 3) |
| `cli_md.rs` | Tests for `builddiag md` command |
| `cli_annotations.rs` | Tests for `builddiag github-annotations` command |
| `cli_diff_aware.rs` | Tests for diff-aware mode and `--always` flag |
| `cli_config.rs` | Tests for configuration file loading |

**Example**:
```rust
/// Test: `builddiag check` with a valid repository passes.
/// _Requirements: 6.1, 7.1_
#[test]
fn check_valid_repository_passes() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = Command::cargo_bin("builddiag").unwrap();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}
```

### Fuzz Tests

Fuzz tests generate random inputs to discover crashes, panics, and unexpected behavior using `cargo-fuzz` with libFuzzer.

**Location**: `fuzz/fuzz_targets/`

**Framework**: [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) + [libfuzzer-sys](https://docs.rs/libfuzzer-sys)

**Prerequisites**:
```bash
# Install cargo-fuzz (requires nightly Rust)
cargo install cargo-fuzz
```

**Running**:
```bash
# List available fuzz targets
cd fuzz && cargo +nightly fuzz list

# Run a specific fuzz target (runs indefinitely until stopped)
cd fuzz && cargo +nightly fuzz run fuzz_version

# Run with time limit (5 minutes)
cd fuzz && cargo +nightly fuzz run fuzz_version -- -max_total_time=300

# Run all fuzz targets with time limits
cd fuzz
for target in fuzz_version fuzz_toml fuzz_checksums fuzz_config; do
    cargo +nightly fuzz run $target -- -max_total_time=300
done
```

**Fuzz Targets**:

| Target | Description |
|--------|-------------|
| `fuzz_version` | Fuzzes `parse_rust_version` function |
| `fuzz_toml` | Fuzzes rust-toolchain.toml parsing |
| `fuzz_checksums` | Fuzzes checksums file parsing |
| `fuzz_config` | Fuzzes Config TOML parsing |

**Handling Crashes**:
When a fuzz target discovers a crash, the crashing input is saved to `fuzz/artifacts/<target>/`. Add regression tests for any discovered crashes:

```bash
# View crash artifacts
ls fuzz/artifacts/

# Reproduce a crash
cd fuzz && cargo +nightly fuzz run fuzz_version fuzz/artifacts/fuzz_version/crash-*
```

### Mutation Tests

Mutation testing verifies that tests detect code changes using `cargo-mutants`. It modifies code (creates "mutants") and checks if tests fail.

**Location**: Configuration in `.cargo/mutants.toml`

**Framework**: [cargo-mutants](https://mutants.rs/)

**Prerequisites**:
```bash
cargo install cargo-mutants
```

**Running**:
```bash
# Run mutation testing on all crates
cargo mutants

# List all mutants without running tests
cargo mutants --list

# Test specific package
cargo mutants -p builddiag-domain

# Run with parallel jobs
cargo mutants -j 4

# Set custom timeout
cargo mutants --timeout 120
```

**Interpreting Results**:

| Status | Meaning |
|--------|---------|
| `killed` | Test suite detected the mutant (good!) |
| `survived` | Test suite did NOT detect the mutant (needs more tests) |
| `timeout` | Mutant caused infinite loop or very slow test |
| `unviable` | Mutant caused compilation error (expected for some mutations) |

**Mutation Score Target**: 90% (killed / (killed + survived))

**Configuration** (`.cargo/mutants.toml`):
- Timeout: 60 seconds per mutant
- Excludes: test files, fuzz targets, xtask, schemas
- Excludes trivial mutations: Display implementations, Default implementations

## Test Organization

### Directory Structure

```
crates/
├── builddiag-types/
│   ├── src/lib.rs              # Inline unit tests (#[cfg(test)])
│   └── tests/
│       └── types_properties.rs  # Property tests for serde round-trips
├── builddiag-domain/
│   ├── src/lib.rs              # Inline unit tests
│   └── tests/
│       └── domain_properties.rs # Property tests for version parsing, summarization
├── builddiag-repo/
│   └── src/lib.rs              # Inline unit tests
├── builddiag-checks/
│   ├── src/lib.rs              # Inline unit tests
│   └── tests/
│       └── check_properties.rs  # Property tests for check behavior
├── builddiag-render/
│   ├── src/lib.rs              # Inline unit tests
│   └── tests/
│       └── render_properties.rs # Property tests for output consistency
├── builddiag-app/
│   └── src/lib.rs              # Inline unit tests
└── builddiag-cli/
    └── tests/
        ├── cli_smoke.rs         # Smoke tests
        ├── cli_check.rs         # Check command tests
        ├── cli_md.rs            # Markdown command tests
        ├── cli_annotations.rs   # GitHub annotations tests
        ├── cli_diff_aware.rs    # Diff-aware mode tests
        ├── cli_config.rs        # Config loading tests
        └── cli_exit_codes.rs    # Exit code tests

fuzz/
├── Cargo.toml
└── fuzz_targets/
    ├── fuzz_version.rs          # Version string fuzzing
    ├── fuzz_toml.rs             # TOML parsing fuzzing
    ├── fuzz_checksums.rs        # Checksums file fuzzing
    └── fuzz_config.rs           # Config file fuzzing
```

### Naming Conventions

| Convention | Example | Usage |
|------------|---------|-------|
| `test_<description>` | `test_config_defaults` | Unit tests |
| `prop_<property_name>` | `prop_config_roundtrip` | Property tests |
| `<command>_<scenario>` | `check_valid_repository_passes` | Integration tests |
| `fuzz_<target>` | `fuzz_version` | Fuzz targets |

### Requirement References

Tests should reference the requirements they validate using comments:

```rust
/// Test: Config defaults are correctly initialized.
/// _Requirements: 2.1_
#[test]
fn test_config_defaults() { ... }
```

For property tests, use the format:
```rust
/// Feature: comprehensive-test-coverage, Property N: <property_name>
/// **Validates: Requirements X.Y**
#[test]
fn prop_<property_name>() { ... }
```

## Running Tests

### Common Commands

```bash
# Run all tests
cargo test --all

# Run tests with verbose output
cargo test --all -- --nocapture

# Run tests matching a pattern
cargo test msrv

# Run ignored tests
cargo test --all -- --ignored

# Run tests in release mode
cargo test --all --release

# Full CI check
cargo run -p xtask -- ci
```

### Test Filtering

```bash
# Run tests in a specific crate
cargo test -p builddiag-domain

# Run a specific test file
cargo test -p builddiag-cli --test cli_check

# Run tests matching a name pattern
cargo test -p builddiag-domain version

# Run a single test
cargo test -p builddiag-domain test_parse_rust_version_two_components
```

## Writing Tests

### Unit Test Guidelines

1. Place unit tests in inline `#[cfg(test)]` modules
2. Use descriptive test names that explain what is being tested
3. Reference requirements in doc comments
4. Test both success and error cases
5. Use `insta` for snapshot testing of complex outputs

### Property Test Guidelines

1. Place property tests in `tests/<crate>_properties.rs`
2. Configure at least 100 iterations: `ProptestConfig::with_cases(100)`
3. Reference the design document property in comments
4. Write smart generators that constrain to valid input space
5. Use `prop_assert!` and `prop_assert_eq!` for assertions

### Integration Test Guidelines

1. Place integration tests in `crates/builddiag-cli/tests/`
2. Use `tempfile::TempDir` for test directories
3. Clean up temporary files after tests
4. Test all exit code scenarios (0, 2, 3)
5. Use `assert_cmd` for CLI testing

### Fuzz Test Guidelines

1. Place fuzz targets in `fuzz/fuzz_targets/`
2. Handle invalid UTF-8 gracefully
3. The function under test should never panic
4. Add regression tests for discovered crashes

## CI Integration

Tests are automatically run in CI via GitHub Actions:

| Job | Trigger | Tests Run |
|-----|---------|-----------|
| `check` | Every PR/push | Format, clippy, unit, property, integration |
| `coverage` | Every PR/push | All tests + coverage report |
| `mutation` | Weekly/manual | Mutation testing |
| `fuzz` | Weekly/manual | Fuzz testing (5 min per target) |

### Coverage Requirements

- **Line coverage target**: 80%
- **Branch coverage target**: 70%
- **Mutation score target**: 90%

See [COVERAGE.md](./COVERAGE.md) for detailed coverage documentation.

## Troubleshooting

### Tests Fail with "command not found"

Ensure the CLI binary is built:
```bash
cargo build -p builddiag-cli
```

### Property Tests Are Slow

Property tests run 100+ iterations by default. For faster iteration during development:
```bash
# Run with fewer cases (not recommended for CI)
PROPTEST_CASES=10 cargo test
```

### Fuzz Tests Won't Start

Fuzz tests require nightly Rust:
```bash
rustup install nightly
cd fuzz && cargo +nightly fuzz run fuzz_version
```

### Mutation Tests Take Too Long

Limit the scope or increase parallelism:
```bash
# Test specific package
cargo mutants -p builddiag-domain

# Increase parallelism
cargo mutants -j 4

# Reduce timeout
cargo mutants --timeout 30
```

### Snapshot Tests Fail After Code Changes

Update snapshots with insta:
```bash
cargo insta review
```

## Further Reading

- [COVERAGE.md](./COVERAGE.md) - Code coverage documentation
- [CONTRIBUTING.md](./CONTRIBUTING.md) - Contribution guidelines
- [proptest book](https://proptest-rs.github.io/proptest/intro.html) - Property testing guide
- [cargo-fuzz book](https://rust-fuzz.github.io/book/) - Fuzz testing guide
- [cargo-mutants docs](https://mutants.rs/) - Mutation testing guide
