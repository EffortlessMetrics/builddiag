# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Profile system for check configurations with three presets: `strict`, `team`, and `oss`.
- `depguard` crate for dependency hygiene checks (wildcard versions, path dependencies, workspace inheritance).
- Comprehensive documentation: architecture, design, implementation, and integration guides.
- Markdown verdict snapshots for report rendering.
- Fuzz testing enhancements with improved coverage.

### Changed
- Report structure refactored with updated JSON schemas.
- Test metadata validation now recognizes workspace inheritance patterns.

### Fixed
- Updated schemars to version 1.2.1 for compatibility.
- Improved string handling in `load_workspace` function.
- CI conditions for mutation and fuzz testing corrected.

## [0.1.0] - 2026-01-31

### Added
- Initial release of builddiag with core crates: builddiag-types, builddiag-domain, builddiag-repo, builddiag-checks, builddiag-render, builddiag-app, builddiag (CLI).
- MSRV validation checks: workspace MSRV definition and member MSRV consistency.
- Toolchain pinning checks: pinned version and MSRV relation validation.
- Checksum verification checks with multiple algorithm support.
- Workspace configuration checks including resolver v2 validation.
- JSON report output for machine-readable results.
- Markdown summary output for PR comments.
- GitHub Actions annotations for CI integration.
