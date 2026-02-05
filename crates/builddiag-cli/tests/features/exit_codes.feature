@exit-codes
Feature: Exit Code Behavior
  As a CI pipeline maintainer
  I want builddiag to return appropriate exit codes
  So that I can correctly detect pass, warn, and fail conditions

  # Exit codes:
  # 0 - Success (all checks pass, or warnings with fail_on: error)
  # 1 - Runtime error (config parse failure, I/O error)
  # 2 - Policy violation (check failed with severity triggering fail_on)

  Background:
    Given a Rust workspace

  Scenario: Valid workspace exits with 0
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  @strict
  Scenario: Missing MSRV with strict profile exits with 2
    Given the workspace has no MSRV defined
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.msrv_defined" "missing_msrv" "error"

  @strict
  Scenario: Missing toolchain with strict profile exits with 2
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has no rust-toolchain.toml
    And the workspace has a checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.toolchain_pinning" "missing_toolchain" "error"

  @strict
  Scenario: Missing checksums with strict profile exits with 2
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has no checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "tools.checksums_file_exists" "missing_checksums" "error"

  Scenario: Relaxed policies allow exit 0
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has no rust-toolchain.toml
    And the workspace has no checksums file
    And the config has toolchain require_pinned "false"
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: fail_on never allows warnings to pass
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the config has fail_on "never"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: fail_on error allows warnings to pass
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the config has fail_on "error"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: Multiple violations still exit with 2
    Given the workspace has no MSRV defined
    And the workspace has no rust-toolchain.toml
    And the workspace has no checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.msrv_defined" "missing_msrv" "error"

  Scenario: Toolchain mismatch with strict profile exits with 2
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.76.0"
    And the workspace has a checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.toolchain_msrv_relation" "toolchain_msrv_mismatch" "error"

  Scenario: Toolchain atleast policy with higher version exits with 0
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.76.0"
    And the workspace has a checksums file
    And the config has toolchain relation_to_msrv "atleast"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: MSRV source any with crate-level MSRV exits with 0
    Given the workspace has MSRV "1.75.0" only in crate
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the config has msrv source "any"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: Valid workspace produces report files
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings
    And the file "artifacts/builddiag/report.json" should exist
    And the file "artifacts/builddiag/comment.md" should exist
