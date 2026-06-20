@catalog
Feature: Check catalog and check listing
  As a developer reading CLI output
  I want to inspect all available checks
  So that I can reason about configured enforcement

  Background:
    Given a Rust workspace

  Scenario: list-checks shows all checks in table format
    When I run builddiag list-checks
    Then the exit code should be 0
    And stdout should contain "rust.msrv_defined"
    And stdout should contain "workspace.publish_ready"
    And stdout should contain "Use 'builddiag explain <check-id>' for detailed documentation."

  Scenario: list-checks strict profile exposes full catalog
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag list-checks with profile "strict"
    Then the exit code should be 0
    And stdout should contain "tools.checksums_file_exists"
    And stdout should contain "deps.security_advisory"
    And stdout should contain "rust.toolchain_msrv_relation"

  Scenario: list-checks strict profile exposes dependency check surface
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag list-checks with profile "strict"
    Then the exit code should be 0
    And stdout should contain "deps.wildcard_version"
    And stdout should contain "deps.lockfile_present"
    And stdout should contain "deps.duplicate_versions"

  Scenario: list-checks filters checks for oss profile
    When I run builddiag list-checks with profile "oss"
    Then the exit code should be 0
    And stdout should contain "rust.msrv_defined"
    And stdout should not contain "tools.checksums_file_exists"
    And stdout should not contain "deps.security_advisory"

  Scenario: list-checks JSON output is machine parsable
    When I run builddiag list-checks with format "json"
    Then the exit code should be 0
    And stdout should contain "\"id\": \"rust.msrv_defined\""
    And stdout should contain "\"id\": \"tools.checksums_file_exists\""
    And stdout should contain "\"profiles\":"
