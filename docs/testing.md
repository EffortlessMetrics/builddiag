# builddiag — Testing Strategy

builddiag is a gatekeeper. The test posture matches that responsibility.

> See also: Each crate's `CLAUDE.md` documents crate-specific testing patterns.

## Test Layers

### 1) Unit Tests (Domain)

Inline `#[cfg(test)]` modules in source files:

- Profile → effective config mapping
- Verdict aggregation
- Deterministic sort key
- Explain registry lookup and completeness

**Location:** `crates/*/src/*.rs`

### 2) Check Tests

Each check has tests covering:
- Pass case (valid input)
- Fail case (invalid input)
- Skip case (missing prerequisite)
- Edge cases (boundary conditions)

**Naming:** `<check>_<scenario>`

Example:
```rust
#[test]
fn msrv_defined_passes_when_workspace_msrv_is_set() { ... }

#[test]
fn msrv_defined_fails_when_workspace_msrv_is_none_and_require_defined_is_true() { ... }
```

### 3) Contract Tests

Tests that enforce invariants across the codebase:

```rust
#[test]
fn contract_every_builtin_check_has_documentation() { ... }

#[test]
fn contract_every_documented_check_is_builtin() { ... }

#[test]
fn contract_no_duplicate_check_ids() { ... }

#[test]
fn contract_no_duplicate_finding_codes() { ... }

#[test]
fn contract_explain_check_resolves_all_check_ids() { ... }

#[test]
fn contract_explain_check_resolves_all_finding_codes() { ... }

#[test]
fn contract_check_ids_follow_naming_convention() { ... }

#[test]
fn contract_finding_codes_are_snake_case() { ... }
```

### 4) Integration Tests

End-to-end tests using `assert_cmd` + `predicates`:

**Location:** `crates/builddiag-cli/tests/`

**Naming:** `<command>_<scenario>`

Example:
```rust
#[test]
fn check_on_valid_repo_returns_pass() { ... }

#[test]
fn check_with_missing_msrv_returns_warn() { ... }
```

### 5) Property Tests

High ROI property tests using `proptest`:

**Location:** `tests/<crate>_properties.rs`

**Naming:** `prop_<property_name>`

Focus areas:
- Sorting is total + stable + deterministic
- Effective config mapping is monotonic and override-safe
- Path normalization produces only repo-relative forward-slash paths
- Version parsing is consistent

### 6) Fuzzing

Focus fuzzing on parsers:
- `Cargo.toml` workspace parsing
- Toolchain file parsing
- Checksum manifest parsing
- Config file parsing

**Location:** `fuzz/fuzz_targets/`

**Rule:** Never panic on malformed input.

### 7) Mutation Testing

Use mutation testing where it's easy to regress semantics silently:
- Severity mapping / exit code mapping
- Effective config application
- Resolver/toolchain/MSRV checks that are "simple but critical"

```bash
cargo mutants                       # Run all mutation tests
cargo mutants -p builddiag-domain   # Test specific package
```

## 8) Conformance Testing

`xtask conform` runs 7 checks against fixture repositories:

| Check | Description |
|-------|-------------|
| **schema** | Validates emitted sensor.report.v1 and builddiag.report.v1 against JSON schemas |
| **determinism** | Runs builddiag twice, asserts byte-identical output |
| **survivability** | Runs builddiag on a broken-config fixture, asserts graceful failure with tool.runtime error |
| **layout** | Validates `--artifacts-dir` produces correct file structure |
| **golden** | Compares output against committed golden files |
| **tool-error** | Validates tool error convention: `check_id="tool.runtime"`, `code="runtime_error"` |
| **library-parity** | Calls `builddiag_core::run()` in-process, compares to CLI subprocess output |

**Fixture auto-discovery:** xtask globs `fixtures/conformance/*/` (skips broken-config and tool-error for schema/golden checks).

**Golden file updates:**
```bash
cargo run -p xtask -- conform --update-golden
```

**Schema validation** uses `contracts/schemas/` for sensor.report.v1 and `schemas/` for builddiag-native schemas.

## Golden Outputs

Maintain golden files for:
- `report.json` (sensor.report.v1)
- `extras/payload.json` (builddiag.report.v1)
- `comment.md`

**Location:** `fixtures/golden/` (per-fixture snapshots)

Determinism is a feature: golden tests make it enforceable.

## CI Conformance

CI must enforce:
- `xtask conform` — all 7 conformance checks
- Schema validation for both `sensor.report.v1` and `builddiag.report.v1`
- Explain coverage for emitted codes
- `cargo fmt --check`
- `cargo clippy -D warnings`

## Running Tests

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p builddiag-domain

# Single test by name
cargo test test_name

# Property tests
cargo test prop_

# Integration tests only
cargo test -p builddiag-cli --test '*'
```

## Test Organization

```
crates/
├── builddiag-types/
│   ├── src/lib.rs                    # Unit tests for types
│   └── tests/types_properties.rs     # Property tests
├── builddiag-domain/
│   ├── src/lib.rs                    # Unit tests for domain logic
│   └── tests/domain_properties.rs    # Property tests
├── builddiag-repo/
│   └── src/lib.rs                    # Unit tests for repo loading
├── builddiag-checks/
│   ├── src/lib.rs                    # Check tests + contract tests
│   └── tests/check_properties.rs     # Property tests
├── builddiag-render/
│   └── src/lib.rs                    # Unit tests for rendering
├── builddiag-app/
│   └── src/lib.rs                    # Unit tests for orchestration
├── builddiag-core/
│   └── src/lib.rs                    # Unit tests for library API + substrate
├── builddiag-cli/
│   └── tests/                        # Integration tests (assert_cmd)
├── depguard/
│   └── src/lib.rs                    # Unit tests for dependency hygiene
fuzz/
│   └── fuzz_targets/                 # Fuzz tests (6 targets)
fixtures/
│   ├── conformance/                  # Conformance test fixtures
│   │   ├── valid-workspace/          # Healthy repo
│   │   ├── missing-msrv/            # Missing MSRV
│   │   ├── all-skip/                # All checks disabled
│   │   ├── broken-config/           # Malformed config (survivability)
│   │   └── tool-error/              # Triggers tool.runtime error
│   └── golden/                       # Golden output files per fixture
contracts/
│   └── schemas/
│       └── sensor.report.v1.schema.json  # Shared sensor schema
schemas/
    ├── builddiag.report.v1.schema.json   # Native report schema
    └── builddiag.config.v1.schema.json   # Config schema
```

## Test Helpers

Common test utilities:

```rust
/// Helper to create a minimal RepoState for testing
fn mock_repo_state() -> RepoState { ... }

/// Helper to create a RepoState with workspace MSRV set
fn mock_repo_with_msrv(msrv: &str) -> RepoState { ... }

/// Helper to create a RepoState with toolchain
fn mock_repo_with_toolchain(channel: &str) -> RepoState { ... }

/// Helper to create a Member
fn mock_member(name: &str, rust_version: Option<&str>, ...) -> Member { ... }
```
