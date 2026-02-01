# Design Document: Comprehensive Test Coverage

## Overview

This design document describes the architecture and implementation approach for achieving comprehensive test coverage of the builddiag Rust CLI tool. The testing strategy employs a multi-layered approach combining unit tests, property-based tests, mutation tests, fuzz tests, integration tests, and BDD-style acceptance tests.

The design follows the existing project conventions: tests in `#[cfg(test)]` modules for unit tests, `tests/` directories for integration tests, and leverages the existing tech stack (insta, assert_cmd, proptest, tempfile).

## Architecture

### Test Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    CI Pipeline (GitHub Actions)                  │
├─────────────────────────────────────────────────────────────────┤
│  Coverage Reports │ Mutation Tests │ Fuzz Tests │ All Tests     │
├─────────────────────────────────────────────────────────────────┤
│                     Integration Tests                            │
│  (CLI commands, end-to-end workflows, exit codes)               │
├─────────────────────────────────────────────────────────────────┤
│                     Acceptance Tests (BDD)                       │
│  (Scenario-based tests for each check type)                     │
├─────────────────────────────────────────────────────────────────┤
│                     Property-Based Tests                         │
│  (Universal invariants, round-trip properties)                  │
├─────────────────────────────────────────────────────────────────┤
│                     Unit Tests                                   │
│  (Individual functions, edge cases, error conditions)           │
└─────────────────────────────────────────────────────────────────┘
```

### Test Distribution by Crate

| Crate | Unit Tests | Property Tests | Integration Tests | Fuzz Targets |
|-------|------------|----------------|-------------------|--------------|
| builddiag-types | Config defaults, serialization | Round-trip serde | - | Config parsing |
| builddiag-domain | Version parsing, status logic | Summarization invariants | - | Version parsing |
| builddiag-repo | File parsing, workspace loading | - | - | TOML parsing, checksums |
| builddiag-checks | All check functions | Check behavior properties | - | - |
| builddiag-render | Markdown, annotations | Output consistency | - | - |
| builddiag-app | Config loading, orchestration | - | - | - |
| builddiag-cli | - | - | All CLI commands | - |

## Components and Interfaces

### 1. Unit Test Module Structure

Each crate will have unit tests organized in inline `#[cfg(test)]` modules:

```rust
// In each crate's lib.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    // Test functions grouped by functionality
    mod version_parsing_tests { ... }
    mod status_determination_tests { ... }
}
```

### 2. Property Test Structure

Property tests will be organized in dedicated test files under `tests/`:

```
crates/
├── builddiag-domain/
│   └── tests/
│       └── domain_properties.rs
├── builddiag-checks/
│   └── tests/
│       └── check_properties.rs  (existing, to be extended)
├── builddiag-render/
│   └── tests/
│       └── render_properties.rs
└── builddiag-types/
    └── tests/
        └── types_properties.rs
```

### 3. Fuzz Test Structure

Fuzz targets will be organized in a dedicated `fuzz/` directory:

```
fuzz/
├── Cargo.toml
└── fuzz_targets/
    ├── fuzz_version_parsing.rs
    ├── fuzz_toml_parsing.rs
    ├── fuzz_checksums_parsing.rs
    └── fuzz_config_parsing.rs
```

### 4. Integration Test Structure

Integration tests in `crates/builddiag-cli/tests/`:

```
crates/builddiag-cli/tests/
├── cli_smoke.rs          (existing)
├── cli_check_command.rs  (new)
├── cli_md_command.rs     (new)
├── cli_annotations.rs    (new)
├── cli_diff_aware.rs     (new)
└── cli_exit_codes.rs     (new)
```

### 5. CI Pipeline Components

```yaml
# .github/workflows/ci.yml structure
jobs:
  test:           # Unit + property + integration tests
  coverage:       # Coverage report generation
  mutation:       # cargo-mutants (scheduled/manual)
  fuzz:           # cargo-fuzz (scheduled)
```

## Data Models

### Test Fixture Types

```rust
/// Standard test fixture for repository state
pub struct TestRepo {
    pub dir: TempDir,
    pub root: Utf8PathBuf,
}

impl TestRepo {
    /// Create a minimal valid workspace
    pub fn minimal_workspace() -> Self { ... }
    
    /// Create workspace with specific MSRV
    pub fn with_msrv(msrv: &str) -> Self { ... }
    
    /// Create workspace with toolchain file
    pub fn with_toolchain(channel: &str) -> Self { ... }
    
    /// Write a file to the test repo
    pub fn write_file(&self, path: &str, content: &str) { ... }
}
```

### Property Test Generators

```rust
/// Generator for valid Rust version strings
fn arb_rust_version() -> impl Strategy<Value = String> {
    prop_oneof![
        (1u32..=2, 50u32..=80).prop_map(|(maj, min)| format!("{}.{}", maj, min)),
        (1u32..=2, 50u32..=80, 0u32..=10).prop_map(|(maj, min, pat)| format!("{}.{}.{}", maj, min, pat)),
    ]
}

/// Generator for valid Config instances
fn arb_config() -> impl Strategy<Value = Config> { ... }

/// Generator for valid Report instances  
fn arb_report() -> impl Strategy<Value = Report> { ... }
```

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

The following properties are derived from the acceptance criteria and represent universal invariants that must hold across all valid inputs.

### Property 1: Config Serialization Round-Trip

*For any* valid Config instance, serializing it to TOML and then parsing it back should produce an equivalent Config.

This property validates that the configuration schema is correctly defined with proper serde attributes and that no data is lost during serialization/deserialization.

```rust
// Pseudocode
forall config: Config where config.is_valid() =>
    toml::from_str(toml::to_string(&config)) == Ok(config)
```

**Validates: Requirements 3.8**

### Property 2: Report Serialization Round-Trip

*For any* valid Report instance, serializing it to JSON and then parsing it back should produce an equivalent Report.

This property validates that the report schema is correctly defined and that JSON output is always valid and parseable.

```rust
// Pseudocode
forall report: Report where report.is_valid() =>
    serde_json::from_str(serde_json::to_string(&report)) == Ok(report)
```

**Validates: Requirements 3.9, 8.5**

### Property 3: Graceful Error Handling

*For any* invalid input (malformed TOML, missing files, invalid version strings), the tool should return an error without panicking.

This property validates that error handling is robust and the tool never crashes on unexpected input.

```rust
// Pseudocode
forall input: InvalidInput =>
    parse(input).is_err() && !panicked()
```

**Validates: Requirements 8.2, 8.3**

### Property 4: Error Messages Contain Context

*For any* error condition, the error message should be non-empty and contain information about what went wrong.

This property validates that errors are actionable and help users understand the problem.

```rust
// Pseudocode
forall error: Error =>
    !error.message().is_empty() && error.message().len() > 10
```

**Validates: Requirements 8.7**

### Property 5: Deterministic Output Ordering

*For any* input, running the tool twice should produce byte-identical output.

This property validates that BTreeMap/BTreeSet are used correctly and output is deterministic.

```rust
// Pseudocode
forall input: ValidInput =>
    run_check(input) == run_check(input)
```

**Validates: Requirements 8.8**

### Property 6: Check Status Consistency

*For any* set of findings, the check status should be consistent with the highest severity finding present.

This property validates the check_status_from_findings logic.

```rust
// Pseudocode
forall findings: Vec<Finding> =>
    if findings.any(|f| f.severity == Error) => status == Fail
    else if findings.any(|f| f.severity == Warn) => status == Warn
    else => status == Pass
```

**Validates: Requirements 2.2 (builddiag-domain unit tests)**

### Property 7: Summary Aggregation Consistency

*For any* set of check reports, the summary counts should equal the sum of individual finding counts.

This property validates the summarize function.

```rust
// Pseudocode
forall checks: Vec<CheckReport> =>
    summary.counts.error == checks.flat_map(|c| c.findings).filter(|f| f.severity == Error).count()
    && summary.counts.warn == checks.flat_map(|c| c.findings).filter(|f| f.severity == Warn).count()
    && summary.counts.info == checks.flat_map(|c| c.findings).filter(|f| f.severity == Info).count()
```

**Validates: Requirements 2.2 (builddiag-domain unit tests)**

### Property 8: Version Parsing Normalization

*For any* valid Rust version string, parsing should produce a normalized three-component semver version.

This property validates the parse_rust_version function handles all valid formats.

```rust
// Pseudocode
forall version_str: ValidVersionString =>
    parse_rust_version(version_str).unwrap().to_string().matches(r"^\d+\.\d+\.\d+$")
```

**Validates: Requirements 3.1 (builddiag-domain property tests)**

## Error Handling

### Error Categories

| Category | Handling Strategy | User Message |
|----------|-------------------|--------------|
| Missing file | Return error with path | "File not found: {path}" |
| Malformed TOML | Return error with parse details | "Failed to parse {path}: {details}" |
| Invalid version | Return error with value | "Invalid version '{value}': {reason}" |
| Permission denied | Return error with path | "Permission denied: {path}" |
| Git command failure | Fail open (continue without diff-aware) | Warning logged |

### Error Propagation

All errors use `anyhow::Result` with context added at each layer:

```rust
fn load_config(path: &Utf8Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read config file: {path}"))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("parse config file: {path}"))?;
    Ok(config)
}
```

## Testing Strategy

### Dual Testing Approach

The testing strategy combines two complementary approaches:

1. **Unit Tests**: Verify specific examples, edge cases, and error conditions
2. **Property Tests**: Verify universal properties across randomly generated inputs

Both are necessary for comprehensive coverage—unit tests catch concrete bugs while property tests verify general correctness.

### Test Framework Selection

| Test Type | Framework | Configuration |
|-----------|-----------|---------------|
| Unit tests | Built-in `#[test]` | Inline in `#[cfg(test)]` modules |
| Snapshot tests | `insta` | JSON snapshots for complex structures |
| Property tests | `proptest` | 100+ iterations per property |
| CLI integration | `assert_cmd` + `predicates` | In `tests/` directory |
| Mutation tests | `cargo-mutants` | Scheduled CI job |
| Fuzz tests | `cargo-fuzz` | Scheduled CI job with time limit |
| Coverage | `cargo-llvm-cov` | Per-PR coverage reports |

### Property-Based Testing Configuration

Each property test must:
- Run minimum 100 iterations (configured via `ProptestConfig`)
- Reference the design document property in a comment
- Use the tag format: `Feature: comprehensive-test-coverage, Property N: {property_text}`

Example:
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

### Test Organization

```
crates/
├── builddiag-types/
│   ├── src/lib.rs              # Inline unit tests
│   └── tests/
│       └── types_properties.rs  # Property tests for serde round-trips
├── builddiag-domain/
│   ├── src/lib.rs              # Inline unit tests (existing)
│   └── tests/
│       └── domain_properties.rs # Property tests for version parsing, summarization
├── builddiag-repo/
│   ├── src/lib.rs              # Inline unit tests (existing)
│   └── tests/
│       └── repo_properties.rs   # Property tests for file parsing
├── builddiag-checks/
│   ├── src/lib.rs              # Inline unit tests (existing)
│   └── tests/
│       └── check_properties.rs  # Property tests (existing, to extend)
├── builddiag-render/
│   ├── src/lib.rs              # Inline unit tests (existing)
│   └── tests/
│       └── render_properties.rs # Property tests for output consistency
├── builddiag-app/
│   ├── src/lib.rs              # Inline unit tests
│   └── tests/
│       └── app_integration.rs   # Integration tests for orchestration
└── builddiag-cli/
    └── tests/
        ├── cli_smoke.rs         # Existing smoke tests
        ├── cli_check.rs         # Check command tests
        ├── cli_md.rs            # Markdown command tests
        ├── cli_annotations.rs   # GitHub annotations tests
        ├── cli_diff_aware.rs    # Diff-aware mode tests
        └── cli_exit_codes.rs    # Exit code scenario tests

fuzz/
├── Cargo.toml
└── fuzz_targets/
    ├── fuzz_version.rs          # Version string fuzzing
    ├── fuzz_toml.rs             # TOML parsing fuzzing
    ├── fuzz_checksums.rs        # Checksums file fuzzing
    └── fuzz_config.rs           # Config file fuzzing
```

### CI Pipeline Design

```yaml
# Workflow structure
jobs:
  test:
    # Fast feedback - runs on every PR
    steps:
      - cargo fmt --check
      - cargo clippy
      - cargo test --all
      
  coverage:
    # Coverage reporting - runs on every PR
    needs: test
    steps:
      - cargo llvm-cov --all --lcov --output-path lcov.info
      - Upload to codecov
      
  mutation:
    # Mutation testing - runs on schedule or manual trigger
    if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'
    steps:
      - cargo mutants --timeout 60
      
  fuzz:
    # Fuzz testing - runs on schedule
    if: github.event_name == 'schedule'
    steps:
      - cargo fuzz run fuzz_version -- -max_total_time=300
      - cargo fuzz run fuzz_toml -- -max_total_time=300
```

### Coverage Thresholds

| Metric | Threshold | Enforcement |
|--------|-----------|-------------|
| Line coverage | 80% | CI fails if below |
| Branch coverage | 70% | Warning if below |
| Mutation score | 90% | Scheduled report |

