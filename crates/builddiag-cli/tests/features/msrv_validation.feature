@msrv
Feature: MSRV Validation
  As a Rust developer
  I want builddiag to validate my MSRV configuration
  So that I can ensure consistent minimum Rust version requirements

  Background:
    Given a Rust workspace

  @strict
  Scenario: Missing MSRV fails with strict profile
    Given the workspace has no MSRV defined
    When I run builddiag check with profile "strict"
    Then the exit code should be 2

  Scenario: MSRV in workspace package passes
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0

  @strict
  Scenario: MSRV only in crate fails with strict profile
    Given the workspace has MSRV "1.75.0" only in crate
    When I run builddiag check with profile "strict"
    Then the exit code should be 2

  Scenario: MSRV in crate passes with source any policy
    Given the workspace has MSRV "1.75.0" only in crate
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the config has msrv source "any"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Multi-crate workspace with consistent MSRV passes
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has crates "a, b, c"
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0

  Scenario: MSRV with matching toolchain passes
    Given the workspace has MSRV "1.76.0" in workspace package
    And the workspace has a pinned toolchain "1.76.0"
    And the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Report contains MSRV findings when missing
    Given the workspace has no MSRV defined
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the file "artifacts/builddiag/report.json" should exist
