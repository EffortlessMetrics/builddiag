# builddiag-repo

Repository discovery and loading layer.

## Purpose

Discovers and parses Cargo workspace information:
- Workspace structure detection (single-crate vs multi-crate)
- Cargo.toml manifest parsing
- Glob pattern expansion for workspace members
- Toolchain and checksum file loading
- Cross-platform path normalization

## Key Types

### Workspace Model
- `WorkspaceModel` - Complete parsed workspace with manifests, members, exclusions
- `WorkspaceInfo` - High-level summary (is_workspace, members, workspace_msrv)
- `Member` - Individual crate info (name, manifest path, MSRV, edition, is_bin)
- `ParsedManifest` - Extracted Cargo.toml data

### Repository State
- `RepoState` - Complete repository information aggregating all sources
- `Toolchain` - Rust toolchain config (path, channel)
- `ToolsChecksums` / `ChecksumEntry` - SHA256 verification data
- `ToolsManifest` / `ToolDecl` - Tool declaration manifest

## Key Functions

### Loading
- `load_repo_state(root)` - Main entry point, loads complete repository info
- `discover_workspace(root)` - Finds all workspace members via glob expansion

### Path Handling
- `normalize_slashes(path)` - Converts to forward slashes (cross-platform)
- `to_repo_relative(abs, root)` - Makes path relative to repository root
- `expand_workspace_patterns(patterns, exclude, root)` - Glob expansion with filtering

## Conventions

- Use `camino::Utf8Path` for all paths
- `BTreeMap` for deterministic member ordering
- Forward slashes in all output paths (even on Windows)
- Support both `[workspace.package]` and legacy per-crate config

## Dependencies

- `builddiag-types`, `builddiag-domain`
- External: anyhow, serde, toml, cargo_metadata, camino, globset

## Testing

- Unit tests for path normalization
- Integration tests with fixture workspaces
- Property tests for glob pattern handling
