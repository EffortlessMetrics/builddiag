# builddiag Context

## Project Overview

`builddiag` is a high-performance, offline-capable Rust tool designed to verify repository build contracts. It analyzes project manifests (e.g., `Cargo.toml`, `rust-toolchain.toml`) and policy configurations to ensure adherence to standards without executing `cargo` commands. It produces:
- Versioned JSON reports (`builddiag.report.v1`, `sensor.report.v1`)
- Markdown summaries for PR comments
- GitHub Actions annotations

It is part of the "Cockpit" CI governance ecosystem, serving as the "repo-truth workspace contract sensor".

## Architecture

The project follows a hexagonal architecture with a layered Cargo workspace structure:

### Workspace Members (`crates/`)
- **`builddiag-cli`**: CLI entry point and argument parsing.
- **`builddiag-core`**: Public library facade (embeddable).
- **`builddiag-app`**: Orchestration, config loading, and output management.
- **`builddiag-checks`**: Implementation of specific validation checks (MSRV, toolchain, etc.).
- **`builddiag-repo`**: Repository state discovery and parsing (I/O adapter).
- **`builddiag-domain`**: Pure domain logic (version parsing, summarization).
- **`builddiag-render`**: Output rendering (Markdown, GitHub annotations).
- **`builddiag-types`**: Shared types, schemas, and profile definitions.
- **`depguard`**: Library for dependency hygiene (integrated by `builddiag-checks`).
- **`xtask`**: Automation scripts for development tasks.

### Data Flow
1.  **Input**: Config loading + Repository discovery (or pre-computed "Substrate").
2.  **Process**: Domain logic applies profiles and runs enabled checks.
3.  **Output**: Aggregated findings are rendered into JSON reports and human-readable formats.

## Development Workflow

### Prerequisites
- **Rust**: Stable toolchain (via `rustup`).
- **Nightly Rust**: Required for fuzz testing (`rustup install nightly`).
- **Tools**: `cargo-mutants`, `cargo-fuzz` (optional but recommended).

### Common Commands

**Building & Running:**
```bash
# Build the project
cargo build

# Run the CLI (development)
cargo run -p builddiag -- check

# Install locally
cargo install --path crates/builddiag-cli
```

**Testing:**
```bash
# Run all tests (Unit, Property, Integration)
cargo test --all

# Run specific crate tests
cargo test -p builddiag-domain

# Run integration tests (CLI)
cargo test -p builddiag-cli

# Run full CI suite (Format, Lint, Test, Schema)
cargo run -p xtask -- ci
```

**Code Quality:**
```bash
# Format code
cargo fmt --all

# Lint (Clippy)
cargo clippy --all-targets --all-features -- -D warnings
```

**Schemas:**
```bash
# Generate JSON schemas (required if types change)
cargo run -p xtask -- schema
```

### Testing Strategy
- **Unit Tests**: Inline `#[cfg(test)]` modules for function-level logic.
- **Property Tests**: `proptest` in `tests/` directories for invariant checking (round-trips, normalization).
- **Integration Tests**: `assert_cmd` in `crates/builddiag-cli/tests/` for end-to-end CLI behavior.
- **Fuzz Tests**: `cargo-fuzz` in `fuzz/` for resilience against malformed inputs.
- **Mutation Tests**: `cargo mutants` to verify test suite quality.

### Conventions
- **Style**: Strictly follow `cargo fmt` and `clippy`.
- **Dependencies**: Dependencies flow downward (CLI -> App -> Checks -> ...). `domain` and `types` should have minimal dependencies.
- **Errors**: Use `anyhow::Result` for fallible operations.
- **Paths**: Prefer `camino::Utf8Path` over `std::path::Path`.
- **Commits**: Clear messages explaining *why*, referencing issues where applicable.

## Key Files
- `Cargo.toml`: Workspace definition.
- `crates/builddiag-cli/src/main.rs`: CLI entry point.
- `builddiag.toml`: Configuration file (optional).
- `CONTRIBUTING.md`: Detailed contribution guidelines.
- `TESTING.md`: Comprehensive testing documentation.
- `docs/architecture.md`: System architecture details.
