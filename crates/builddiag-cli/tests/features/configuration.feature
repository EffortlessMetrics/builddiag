@config
Feature: Configuration Loading
  As a builddiag user
  I want to configure check behavior via config files and CLI flags
  So that I can customize validation to my project's needs

  Background:
    Given a Rust workspace
    And the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"

  Scenario: Default config produces artifacts in default location
    Given the workspace has a checksums file
    When I run builddiag check
    Then the exit code should be 0
    And the file "artifacts/builddiag/report.json" should exist
    And the file "artifacts/builddiag/comment.md" should exist

  Scenario: Custom config file is loaded
    Given the workspace has no checksums file
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Named config file is loaded
    Given the workspace has no checksums file
    And the config file is named "custom-config.toml"
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Custom output directory from config
    Given the workspace has a checksums file
    And the config has out_dir "custom-output"
    When I run builddiag check
    Then the exit code should be 0
    And the file "custom-output/report.json" should exist
    And the file "custom-output/comment.md" should exist

  Scenario: Profile strict enables all strict checks
    Given the workspace has a checksums file
    When I run builddiag check with profile "strict"
    Then the exit code should be 0

  Scenario: Profile oss has relaxed checks
    Given the workspace has no checksums file
    When I run builddiag check with profile "oss"
    Then the exit code should be 0

  Scenario: Config can disable toolchain pinning requirement
    Given the workspace has an unpinned toolchain "stable"
    And the workspace has a checksums file
    And the config has toolchain require_pinned "false"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Config can change toolchain MSRV relation
    Given the workspace has a pinned toolchain "1.76.0"
    And the workspace has a checksums file
    And the config has toolchain relation_to_msrv "atleast"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Config can disable checksums requirement
    Given the workspace has no checksums file
    And the config has checksums require_file "false"
    When I run builddiag check
    Then the exit code should be 0

  Scenario: Config can change MSRV source policy
    Given the workspace has MSRV "1.75.0" only in crate
    And the workspace has a checksums file
    And the config has msrv source "any"
    When I run builddiag check
    Then the exit code should be 0
