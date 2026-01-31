# Tech Stack

## Language & Edition
- Rust 2021 edition
- Dual licensed: MIT OR Apache-2.0

## Key Dependencies
- `clap` - CLI argument parsing with derive macros
- `serde` / `serde_json` / `toml` - serialization
- `schemars` - JSON Schema generation
- `cargo_metadata` - parsing Cargo.toml via cargo
- `camino` - UTF-8 path handling
- `anyhow` / `thiserror` - error handling
- `chrono` - timestamps
- `globset` - glob pattern matching
- `sha2` / `hex` - checksum verification
- `semver` - version parsing

## Testing
- `insta` - snapshot testing
- `assert_cmd` / `predicates` - CLI integration tests
- `tempfile` - temporary directories for tests

## Common Commands

```bash
# Build
cargo build

# Run all tests
cargo test --all

# Run CLI
cargo run -p builddiag -- check

# Install locally
cargo install --path crates/builddiag-cli

# Generate JSON schemas
cargo run -p xtask -- schema

# Full CI check (fmt, clippy, test, schema)
cargo run -p xtask -- ci

# Format code
cargo fmt --all

# Lint
cargo clippy --all-targets --all-features -- -D warnings
```

## Code Style
- Use `anyhow::Result` for fallible functions
- Prefer `camino::Utf8Path` over `std::path::Path`
- Derive `Serialize`, `Deserialize`, `JsonSchema` for public types
- Use `BTreeMap`/`BTreeSet` for deterministic ordering in outputs
