# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

## [Unreleased]

- Initial unreleased changes

## [0.1.0] - 2026-01-31

### Added
- Initial public release of builddiag with core crates: builddiag-types, builddiag-domain, builddiag-repo, builddiag-checks, builddiag-render, builddiag-app, builddiag (CLI).
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-01-31

### Added
- Initial release
- MSRV validation checks
  - Check that MSRV is defined in workspace Cargo.toml
  - Check that member crate MSRVs are consistent with workspace MSRV
- Toolchain pinning checks
  - Check that rust-toolchain.toml pins a specific version
  - Check that toolchain version satisfies MSRV requirements
- Checksum verification checks
  - Verify tool checksums against expected values
  - Support for multiple checksum algorithms
- Workspace configuration checks
  - Validate workspace resolver is set to "2"
- JSON report output for machine-readable results
- Markdown summary output for PR comments
- GitHub Actions annotations for CI integration
