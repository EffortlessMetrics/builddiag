# Implementation Plan: Release-Ready

## Overview

This plan implements the release-ready infrastructure for builddiag: CI/CD workflows, crate metadata, changelog, test coverage, documentation, and contributing guide. Tasks are ordered to establish CI first, then metadata and docs, then expanded test coverage.

## Tasks

- [x] 1. Create CI workflow
  - [x] 1.1 Create `.github/workflows/ci.yml` with fmt, clippy, test, and schema validation steps
    - Trigger on push to main and pull requests
    - Use ubuntu-latest runner with stable Rust toolchain
    - Add Swatinem/rust-cache for dependency caching
    - Include schema validation step that runs xtask schema and checks for git diff
    - _Requirements: 1.1, 1.2, 1.4, 1.5_

- [x] 2. Create release workflow
  - [x] 2.1 Create `.github/workflows/release.yml` for crates.io publishing
    - Trigger on tags matching `v*.*.*`
    - Publish crates in dependency order with delays between publishes
    - Use CARGO_REGISTRY_TOKEN secret for authentication
    - _Requirements: 2.1, 2.2, 2.3_

- [x] 3. Update crate metadata for crates.io
  - [x] 3.1 Add metadata to `builddiag-types/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.2 Add metadata to `builddiag-domain/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.3 Add metadata to `builddiag-repo/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.4 Add metadata to `builddiag-checks/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.5 Add metadata to `builddiag-render/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.6 Add metadata to `builddiag-app/Cargo.toml`
    - Add description, repository, homepage, keywords, categories
    - _Requirements: 3.1, 3.2_
  - [x] 3.7 Add metadata to CLI crate `builddiag/Cargo.toml`
    - Add description, repository, homepage, readme
    - Add keywords: rust, cli, build, validation, msrv
    - Add categories: development-tools, command-line-utilities
    - _Requirements: 3.1, 3.2, 3.3, 3.4_
  - [x] 3.8 Write property test for crate metadata completeness
    - **Property 1: Crate Metadata Completeness**
    - Verify all workspace crates have required metadata fields
    - **Validates: Requirements 3.1, 3.2**

- [x] 4. Create CHANGELOG
  - [x] 4.1 Create `CHANGELOG.md` following keep-a-changelog format
    - Include Unreleased section at top
    - Document 0.1.0 release with all current features (MSRV checks, toolchain checks, checksum checks, workspace checks, JSON/Markdown output, GitHub annotations)
    - _Requirements: 4.1, 4.2, 4.3_

- [x] 5. Checkpoint - Verify CI and metadata
  - Ensure all tests pass, ask the user if questions arise.
  - Verify CI workflow syntax is valid
  - Verify crate metadata is complete

- [x] 6. Add unit tests for builddiag-checks
  - [x] 6.1 Add tests for `check_msrv_defined`
    - Test pass case: workspace_msrv is set
    - Test fail case: workspace_msrv is None and require_defined is true
    - _Requirements: 5.1, 5.2_
  - [x] 6.2 Add tests for `check_msrv_consistent`
    - Test pass case: all members match workspace MSRV
    - Test fail case: member has different MSRV
    - Test skip case: no workspace MSRV to compare
    - _Requirements: 5.1, 5.2_
  - [x] 6.3 Add tests for `check_toolchain_pinning`
    - Test pass case: toolchain pinned to specific version
    - Test fail case: toolchain is "stable" (unpinned)
    - Test fail case: missing toolchain file when required
    - _Requirements: 5.1, 5.2_
  - [x] 6.4 Add tests for `check_toolchain_msrv_relation`
    - Test pass case: toolchain equals MSRV
    - Test fail case: toolchain less than MSRV
    - Test skip case: non-numeric toolchain channel
    - _Requirements: 5.1, 5.2_
  - [x] 6.5 Add tests for `check_workspace_resolver`
    - Test pass case: resolver = "2"
    - Test fail case: resolver missing or not "2"
    - Test skip case: not a workspace
    - _Requirements: 5.1, 5.2_
  - [x] 6.6 Write property test for check pass behavior
    - **Property 2: Check Pass Behavior**
    - For valid inputs, Pass status means no Error severity findings
    - **Validates: Requirements 5.1**
  - [x] 6.7 Write property test for check fail behavior
    - **Property 3: Check Fail Behavior**
    - For invalid inputs, findings have non-empty messages
    - **Validates: Requirements 5.2**

- [x] 7. Add unit tests for builddiag-repo parsing
  - [x] 7.1 Add tests for `parse_checksums`
    - Test valid checksums file parsing
    - Test handling of comments and empty lines
    - _Requirements: 5.4_
  - [x] 7.2 Add tests for toolchain file parsing
    - Test `rust-toolchain.toml` format
    - Test legacy `rust-toolchain` format
    - _Requirements: 5.4_

- [x] 8. Add doc comments to public APIs
  - [x] 8.1 Add doc comments to `builddiag-types` public types
    - Document Report, Config, Finding, CheckReport, Severity, CheckStatus
    - Add module-level documentation
    - _Requirements: 6.1_
  - [x] 8.2 Add doc comments to `builddiag-domain` public functions
    - Document parse_rust_version, check_status_from_findings, summarize
    - _Requirements: 6.2_
  - [x] 8.3 Verify `cargo doc` runs without warnings
    - Run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
    - _Requirements: 6.4_

- [x] 9. Create CONTRIBUTING.md
  - [x] 9.1 Create `CONTRIBUTING.md` with development setup and guidelines
    - Explain development environment setup (Rust toolchain, clone, build)
    - Document project structure and crate responsibilities
    - Explain PR review process and coding standards
    - Reference xtask commands (ci, schema, fmt, clippy)
    - _Requirements: 7.1, 7.2, 7.3, 7.4_

- [x] 10. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.
  - Run `cargo test --all`
  - Run `cargo clippy --all-targets --all-features -- -D warnings`
  - Run `cargo doc --no-deps`

## Notes

- CI workflow should be created first to validate subsequent changes
- Crate metadata must be complete before release workflow can publish
- Property tests validate universal correctness properties
- Unit tests validate specific examples and edge cases
