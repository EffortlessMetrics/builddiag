# Code Coverage Guide

This document describes how to generate and interpret code coverage reports for the builddiag project.

## Prerequisites

Install `cargo-llvm-cov`:

```bash
# Install LLVM tools (required for coverage instrumentation)
rustup component add llvm-tools-preview

# Install cargo-llvm-cov
cargo install cargo-llvm-cov
```

## Running Coverage

### Quick Start

Generate an LCOV report (for CI integration):

```bash
cargo run -p xtask -- coverage
```

This creates `coverage/lcov.info` which can be uploaded to codecov or coveralls.

### HTML Report

Generate an HTML report for local viewing:

```bash
cargo run -p xtask -- coverage --html
```

This creates `coverage/html/index.html` which you can open in a browser.

### Open in Browser

Generate and automatically open the HTML report:

```bash
cargo run -p xtask -- coverage --html --open
```

### Custom Output Directory

Specify a custom output directory:

```bash
cargo run -p xtask -- coverage --out-dir my-coverage
```

### Direct cargo-llvm-cov Commands

You can also run cargo-llvm-cov directly for more control:

```bash
# Summary in terminal
cargo llvm-cov --workspace

# LCOV format
cargo llvm-cov --workspace --lcov --output-path coverage.lcov

# HTML report
cargo llvm-cov --workspace --html --output-dir coverage-html

# JSON format (for programmatic analysis)
cargo llvm-cov --workspace --json --output-path coverage.json

# Show uncovered lines
cargo llvm-cov --workspace --show-missing-lines
```

## Coverage Targets

Based on the comprehensive test coverage specification:

| Metric | Target | Enforcement |
|--------|--------|-------------|
| Line coverage | 80% | CI fails if below |
| Branch coverage | 70% | Warning if below |

## Expected Coverage by Crate

The builddiag workspace consists of several crates with different coverage expectations:

### builddiag-types
- **Expected coverage**: High (90%+)
- **Key areas**: Config defaults, serialization, type constructors
- **Uncovered paths**: Rarely-used config combinations

### builddiag-domain
- **Expected coverage**: High (90%+)
- **Key areas**: Version parsing, status determination, summarization
- **Uncovered paths**: Edge cases in version string parsing

### builddiag-repo
- **Expected coverage**: Medium-High (80%+)
- **Key areas**: File parsing, workspace loading, toolchain detection
- **Uncovered paths**: Error handling for rare file system conditions

### builddiag-checks
- **Expected coverage**: High (85%+)
- **Key areas**: All check implementations (MSRV, toolchain, checksums, resolver)
- **Uncovered paths**: Complex policy override combinations

### builddiag-render
- **Expected coverage**: High (85%+)
- **Key areas**: Markdown generation, GitHub annotation formatting
- **Uncovered paths**: Edge cases in output formatting

### builddiag-app
- **Expected coverage**: Medium (75%+)
- **Key areas**: Config loading, orchestration, file writing
- **Uncovered paths**: Error recovery paths, atomic write failures

### builddiag-cli
- **Expected coverage**: Medium (70%+)
- **Key areas**: Argument parsing, command dispatch
- **Uncovered paths**: Covered primarily by integration tests

## Identifying Uncovered Code

### Using HTML Report

1. Generate HTML report: `cargo run -p xtask -- coverage --html --open`
2. Navigate to specific crates in the report
3. Red-highlighted lines indicate uncovered code
4. Yellow-highlighted lines indicate partially covered branches

### Using Terminal Output

Show uncovered lines directly:

```bash
cargo llvm-cov --workspace --show-missing-lines
```

### Common Uncovered Patterns

The following patterns are commonly uncovered and may be acceptable:

1. **Error handling paths**: Rare error conditions that are difficult to trigger in tests
2. **Platform-specific code**: Code paths for other operating systems
3. **Debug/Display implementations**: Formatting code used only for debugging
4. **Unreachable code**: Match arms that should never be reached

## CI Integration

### GitHub Actions

The CI pipeline uploads coverage to codecov:

```yaml
- name: Generate coverage
  run: cargo run -p xtask -- coverage

- name: Upload to codecov
  uses: codecov/codecov-action@v4
  with:
    files: coverage/lcov.info
    fail_ci_if_error: true
```

### Codecov Configuration

The `codecov.yml` file configures coverage thresholds:

```yaml
coverage:
  status:
    project:
      default:
        target: 80%
        threshold: 2%
    patch:
      default:
        target: 80%
```

## Excluding Code from Coverage

Some code should be excluded from coverage calculations:

### Test Code

Test code is automatically excluded by cargo-llvm-cov.

### Generated Code

If you have generated code, exclude it with:

```rust
#[cfg(not(coverage))]
mod generated;
```

### Specific Functions

Exclude specific functions:

```rust
#[cfg_attr(coverage, ignore)]
fn rarely_used_function() { ... }
```

## Troubleshooting

### "cargo-llvm-cov is not installed"

Install it with:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

### Low Coverage Numbers

1. Check if tests are actually running: `cargo test --all`
2. Ensure property tests are included (they may be slow)
3. Check for `#[ignore]` attributes on tests

### Coverage Report is Empty

1. Ensure tests exist and pass
2. Check that the workspace is correctly configured
3. Try running with `--verbose` flag

## Further Reading

- [cargo-llvm-cov documentation](https://github.com/taiki-e/cargo-llvm-cov)
- [LLVM Source-based Code Coverage](https://clang.llvm.org/docs/SourceBasedCodeCoverage.html)
- [Codecov documentation](https://docs.codecov.com/)
