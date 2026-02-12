@toolchain
Feature: Toolchain Pinning Validation
  As a Rust developer
  I want builddiag to validate my rust-toolchain.toml
  So that my builds are reproducible with a pinned Rust version

  Background:
    Given a Rust workspace
    And the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a checksums file

  @strict
  Scenario: Missing rust-toolchain.toml fails with strict profile
    Given the workspace has no rust-toolchain.toml
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.toolchain_pinning" "missing_toolchain" "error"

  @strict
  Scenario: Unpinned toolchain using stable fails with strict profile
    Given the workspace has an unpinned toolchain "stable"
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.toolchain_pinning" "unpinned_channel" "error"

  @strict
  Scenario: Toolchain version mismatch fails with strict profile
    Given the workspace has a pinned toolchain "1.76.0"
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the report should exist at the canonical path
    And the report verdict should be "fail"
    And the report should include finding "rust.toolchain_msrv_relation" "toolchain_msrv_mismatch" "error"

  Scenario: Toolchain matching MSRV passes
    Given the workspace has a pinned toolchain "1.75.0"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: Toolchain greater than MSRV passes with atleast policy
    Given the workspace has a pinned toolchain "1.76.0"
    And the config has toolchain relation_to_msrv "atleast"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings

  Scenario: Unpinned toolchain passes when not required
    Given the workspace has an unpinned toolchain "stable"
    And the config has toolchain require_pinned "false"
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the report verdict should be "pass"
    And the report should not include any "error" findings
