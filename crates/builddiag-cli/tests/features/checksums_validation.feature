@checksums
Feature: Checksums File Validation
  As a Rust developer
  I want builddiag to validate my tool checksums
  So that I can verify third-party tool integrity

  Background:
    Given a Rust workspace
    And the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"

  @strict
  Scenario: Missing checksums file fails with strict profile
    Given the workspace has no checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 2

  Scenario: Missing checksums file passes when not required
    Given the workspace has no checksums file
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0
