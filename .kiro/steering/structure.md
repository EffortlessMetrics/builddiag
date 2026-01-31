# Project Structure

Cargo workspace with layered architecture. Dependencies flow downward only.

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

## Dependency Graph

```
cli → app → render
         → checks → repo → domain → types
                        → types
                  → domain
                  → types
         → repo
         → types
```

## Other Directories

- `xtask/` - Development automation (schema generation, CI)
- `schemas/` - Generated JSON schemas for report and config

## Conventions

- Each crate has a single `lib.rs` (or `main.rs` for CLI)
- Tests live in `tests/` subdirectory or inline `#[cfg(test)]` modules
- CLI integration tests in `crates/builddiag-cli/tests/`
- Snapshot tests use `insta` crate
