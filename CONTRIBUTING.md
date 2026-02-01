# Contributing to builddiag

Thank you for your interest in contributing to builddiag! This document provides guidelines and information for contributors.

## Development Environment Setup

### Prerequisites

- **Rust toolchain**: Install via [rustup](https://rustup.rs/)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Stable Rust**: The project uses Rust 2021 edition with stable toolchain
  ```bash
  rustup default stable
  ```

### Getting Started

1. **Clone the repository**
   ```bash
   git clone https://github.com/user/builddiag.git
   cd builddiag
   ```

2. **Build the project**
   ```bash
   cargo build
   ```

3. **Run tests**
   ```bash
   cargo test --all
   ```

4. **Install locally** (optional)
   ```bash
   cargo install --path crates/builddiag-cli
   ```

## Project Structure

builddiag uses a layered Cargo workspace architecture. Dependencies flow downward only.

```
crates/
├── builddiag-cli/      # CLI entry point (binary crate: "builddiag")
├── builddiag-app/      # Application orchestration, config loading, output writing
├── builddiag-render/   # Markdown and GitHub annotation rendering
├── builddiag-checks/   # Check implementations (MSRV, toolchain, checksums, etc.)
├── builddiag-repo/     # Repository state loading (Cargo.toml, rust-toolchain, etc.)
├── builddiag-domain/   # Core domain logic (version parsing, summarization)
└── builddiag-types/    # Shared types, config schema, report schema
```

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `builddiag-cli` | Command-line interface, argument parsing with clap |
| `builddiag-app` | Orchestrates checks, loads config, writes output files |
| `builddiag-render` | Generates Markdown summaries and GitHub annotations |
| `builddiag-checks` | Implements validation checks (MSRV, toolchain, checksums, workspace) |
| `builddiag-repo` | Parses repository files (Cargo.toml, rust-toolchain.toml, checksums) |
| `builddiag-domain` | Core logic for version parsing and result summarization |
| `builddiag-types` | Shared types: Report, Config, Finding, CheckReport, Severity |

### Dependency Graph

```
cli → app → render
         → checks → repo → domain → types
                        → types
                  → domain
                  → types
         → repo
         → types
```

## Development Commands

The project uses `xtask` for common development tasks:

```bash
# Run full CI check (format, lint, test, schema validation)
cargo run -p xtask -- ci

# Generate JSON schemas
cargo run -p xtask -- schema

# Format code
cargo fmt --all

# Lint with clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
cargo test --all

# Run the CLI
cargo run -p builddiag -- check
```

## Coding Standards

### Code Style

- Use `cargo fmt` to format all code before committing
- All clippy warnings must be resolved (`-D warnings` flag)
- Use `anyhow::Result` for fallible functions
- Prefer `camino::Utf8Path` over `std::path::Path`
- Derive `Serialize`, `Deserialize`, `JsonSchema` for public types
- Use `BTreeMap`/`BTreeSet` for deterministic ordering in outputs

### Documentation

- Add doc comments (`///`) to all public types and functions
- Include examples in doc comments where appropriate
- Run `cargo doc --no-deps` to verify documentation builds without warnings

### Testing

- Write unit tests for new functionality
- Use `insta` for snapshot testing
- Use `assert_cmd` and `predicates` for CLI integration tests
- Use `tempfile` for tests requiring temporary directories
- Tests should be placed in `tests/` subdirectory or inline `#[cfg(test)]` modules

## Pull Request Process

### Before Submitting

1. **Run the full CI check locally**
   ```bash
   cargo run -p xtask -- ci
   ```
   This runs format check, clippy, all tests, and schema validation.

2. **Ensure schemas are up-to-date**
   ```bash
   cargo run -p xtask -- schema
   git diff schemas/
   ```
   If there are changes, commit them with your PR.

3. **Add tests** for new functionality or bug fixes

4. **Update documentation** if you're changing public APIs

### PR Guidelines

- Keep PRs focused on a single change or feature
- Write clear commit messages describing what and why
- Reference any related issues in the PR description
- Ensure all CI checks pass before requesting review

### Review Process

1. Submit your PR against the `main` branch
2. CI will automatically run format, lint, and test checks
3. A maintainer will review your changes
4. Address any feedback and push updates
5. Once approved, a maintainer will merge your PR

## Reporting Issues

When reporting bugs, please include:

- builddiag version (`builddiag --version`)
- Rust version (`rustc --version`)
- Operating system
- Steps to reproduce the issue
- Expected vs actual behavior

## License

By contributing to builddiag, you agree that your contributions will be licensed under the MIT OR Apache-2.0 license.
