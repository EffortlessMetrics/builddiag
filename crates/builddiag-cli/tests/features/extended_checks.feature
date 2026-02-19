@extended-checks
Feature: Extended Check Coverage
  As a builddiag adopter
  I want newer checks covered by BDD
  So that regressions are caught at the CLI contract layer

  Background:
    Given a Rust workspace
    And the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file

  Scenario: Publish-ready check fails when required metadata is missing
    Given the crate is missing publish metadata
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "workspace.publish_ready" "missing_description" "error"
    And the report should include finding "workspace.publish_ready" "missing_license" "error"

  Scenario: Publish-ready check skips publish-disabled crates
    Given the crate is publish-disabled and missing publish metadata
    When I run builddiag check with profile "strict"
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include finding "workspace.publish_ready" "missing_description"
    And the report should not include finding "workspace.publish_ready" "missing_license"

  Scenario: Edition deprecations check fails on outdated edition in strict profile
    Given the workspace edition is "2015"
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.edition_deprecations" "deprecated_edition" "error"

  Scenario: Duplicate dependency versions are detected across workspace members
    Given the workspace has conflicting serde versions across members
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "deps.duplicate_versions" "duplicate_dependency_version" "error"

  Scenario: Lockfile check fails for binary crates without Cargo.lock
    Given the crate has a binary target
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "deps.lockfile_present" "missing_lockfile_for_binary" "error"

  Scenario: Lockfile check passes for binary crates when Cargo.lock exists
    Given the crate has a binary target
    And the workspace has a Cargo.lock file
    When I run builddiag check with profile "strict"
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include finding "deps.lockfile_present" "missing_lockfile_for_binary"
