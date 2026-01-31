# Requirements Document

## Introduction

This document specifies the requirements for making the builddiag Rust CLI tool release-ready. The tool validates the "build contract" of Rust repositories through static analysis of manifests and policy files. The core functionality is implemented across 7 crates with a layered architecture. This feature focuses on establishing CI/CD infrastructure, release automation, documentation, and test coverage needed for public release to crates.io.

## Glossary

- **CI_Workflow**: GitHub Actions workflow that runs on pull requests and pushes to validate code quality
- **Release_Workflow**: GitHub Actions workflow that publishes crates to crates.io on version tags
- **Crate_Metadata**: Package metadata in Cargo.toml files required for crates.io publishing
- **CHANGELOG**: A file documenting notable changes between versions following keep-a-changelog format
- **Doc_Comments**: Rust documentation comments (///) that generate API documentation
- **Check_Module**: One of the 9 check implementations in builddiag-checks (MSRV, toolchain, checksums, etc.)

## Requirements

### Requirement 1: CI Workflow

**User Story:** As a maintainer, I want automated CI checks on every pull request and push, so that code quality is validated before merging.

#### Acceptance Criteria

1. WHEN code is pushed to main or a pull request is opened, THE CI_Workflow SHALL run cargo fmt check, clippy, and all tests
2. WHEN the CI_Workflow runs, THE CI_Workflow SHALL validate that JSON schemas are up-to-date by running xtask schema and checking for uncommitted changes
3. WHEN any CI check fails, THE CI_Workflow SHALL report the failure and block merging
4. THE CI_Workflow SHALL run on ubuntu-latest with stable Rust toolchain
5. THE CI_Workflow SHALL cache cargo dependencies to speed up subsequent runs

### Requirement 2: Release Workflow

**User Story:** As a maintainer, I want automated publishing to crates.io when I create a version tag, so that releases are consistent and reproducible.

#### Acceptance Criteria

1. WHEN a tag matching pattern v*.*.* is pushed, THE Release_Workflow SHALL publish all workspace crates to crates.io in dependency order
2. THE Release_Workflow SHALL publish crates in order: builddiag-types, builddiag-domain, builddiag-repo, builddiag-checks, builddiag-render, builddiag-app, builddiag (CLI)
3. WHEN publishing to crates.io, THE Release_Workflow SHALL use the CARGO_REGISTRY_TOKEN secret for authentication
4. IF a crate publish fails, THEN THE Release_Workflow SHALL stop and report the error

### Requirement 3: Crate Metadata

**User Story:** As a user discovering builddiag on crates.io, I want complete package metadata, so that I can understand what the tool does and find its source.

#### Acceptance Criteria

1. THE Crate_Metadata SHALL include description, repository, homepage, and readme fields for all crates
2. THE Crate_Metadata SHALL include keywords and categories appropriate for each crate
3. THE Crate_Metadata for the CLI crate SHALL include keywords: rust, cli, build, validation, msrv
4. THE Crate_Metadata for the CLI crate SHALL include categories: development-tools, command-line-utilities

### Requirement 4: CHANGELOG

**User Story:** As a user upgrading builddiag, I want a changelog documenting version changes, so that I can understand what changed between releases.

#### Acceptance Criteria

1. THE CHANGELOG SHALL follow the keep-a-changelog format with sections: Added, Changed, Deprecated, Removed, Fixed, Security
2. THE CHANGELOG SHALL include an Unreleased section at the top for tracking upcoming changes
3. THE CHANGELOG SHALL document the initial 0.1.0 release with all current features

### Requirement 5: Test Coverage for Checks

**User Story:** As a maintainer, I want comprehensive tests for check implementations, so that I can refactor with confidence.

#### Acceptance Criteria

1. WHEN a Check_Module validates a passing condition, THE Check_Module SHALL return findings with appropriate severity
2. WHEN a Check_Module validates a failing condition, THE Check_Module SHALL return findings describing the issue
3. THE builddiag-checks crate SHALL have unit tests covering success and failure cases for each check type
4. THE builddiag-repo crate SHALL have unit tests for parsing Cargo.toml and rust-toolchain.toml files

### Requirement 6: API Documentation

**User Story:** As a developer using builddiag as a library, I want doc comments on public APIs, so that I can understand how to use the types and functions.

#### Acceptance Criteria

1. THE Doc_Comments SHALL be present on all public types in builddiag-types
2. THE Doc_Comments SHALL be present on all public functions in builddiag-domain
3. THE Doc_Comments SHALL include examples where appropriate for complex APIs
4. WHEN cargo doc is run, THE Doc_Comments SHALL generate documentation without warnings

### Requirement 7: Contributing Guide

**User Story:** As a potential contributor, I want a CONTRIBUTING.md file, so that I understand how to contribute to the project.

#### Acceptance Criteria

1. THE CONTRIBUTING.md SHALL explain how to set up the development environment
2. THE CONTRIBUTING.md SHALL document the project structure and crate responsibilities
3. THE CONTRIBUTING.md SHALL explain the PR review process and coding standards
4. THE CONTRIBUTING.md SHALL reference the existing xtask commands for common development tasks
