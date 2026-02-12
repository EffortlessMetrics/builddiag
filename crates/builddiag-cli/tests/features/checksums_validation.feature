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
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "tools.checksums_file_exists" "missing_checksums" "error"

  Scenario: Missing checksums file passes when not required
    Given the workspace has no checksums file
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings
