@receipt
Feature: Receipt Contract
  As a CI integrator
  I want builddiag receipts to follow the declared contracts
  So that downstream tooling can parse and trust the output

  Background:
    Given a Rust workspace

  Scenario: Sensor format emits a valid sensor.report.v1 envelope
    Given the workspace has no MSRV defined
    And the output format is "sensor"
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the sensor report should exist at the canonical path
    And the sensor report verdict status should be "fail"
    And the report should include finding "rust.msrv_defined" "missing_msrv" "error"

  Scenario: Cockpit artifacts include sensor envelope and payload reference
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the check mode is "cockpit"
    And artifacts are written to "ci-artifacts"
    When I run builddiag check
    Then the exit code should be 0
    And the sensor report should exist at the canonical path
    And the sensor report verdict status should be "pass"
    And the file "ci-artifacts/extras/payload.json" should exist
    And the file "ci-artifacts/comment.md" should exist
    And the sensor report should include artifact "payload" at "extras/payload.json"
    And the sensor report should include artifact "comment" at "comment.md"
    And the file "ci-artifacts/extras/payload.json" should have schema "builddiag.report.v1"

  Scenario: Sensor verdict exposes failed-check reason and data
    Given the workspace has no MSRV defined
    And the output format is "sensor"
    When I run builddiag check with profile "strict"
    Then the exit code should be 2
    And the sensor report should exist at the canonical path
    And the sensor report verdict status should be "fail"
    And the sensor report verdict should include reason "checks_failed"
    And the sensor report verdict data should include "rust.msrv_defined" in "failed_checks"

  Scenario: Sensor verdict exposes warned-check reason and data
    Given the workspace has no MSRV defined
    And the output format is "sensor"
    When I run builddiag check
    Then the exit code should be 0
    And the sensor report should exist at the canonical path
    And the sensor report verdict status should be "warn"
    And the sensor report verdict should include reason "checks_warned"
    And the sensor report verdict data should include "rust.msrv_defined" in "warned_checks"

  Scenario: Sensor run captures capability states
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has no rust-toolchain.toml
    And the workspace has no checksums file
    And the config has toolchain require_pinned "false"
    And the config has checksums require_file "false"
    And the output format is "sensor"
    When I run builddiag check
    Then the exit code should be 0
    And the sensor report should exist at the canonical path
    And the sensor report should include capabilities "git, config, toolchain, checksums, diff_aware"
    And the sensor capability "toolchain" should be "unavailable"
    And the sensor capability "checksums" should be "skipped"
    And the sensor capability "diff_aware" should be "skipped"

  Scenario: Cockpit mode defaults to canonical artifact layout
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And the check mode is "cockpit"
    When I run builddiag check
    Then the exit code should be 0
    And the sensor report should exist at the canonical path
    And the sensor report verdict status should be "pass"
    And the file "artifacts/builddiag/extras/payload.json" should exist
    And the file "artifacts/builddiag/comment.md" should exist
    And the sensor report should include artifact "payload" at "extras/payload.json"
    And the sensor report should include artifact "comment" at "comment.md"
    And the file "artifacts/builddiag/extras/payload.json" should have schema "builddiag.report.v1"

  Scenario: Cockpit mode writes an error receipt on invalid config
    Given the check mode is "cockpit"
    And the config file is invalid
    When I run builddiag check
    Then the exit code should be 0
    And the report should exist at the canonical path
    And the file "artifacts/builddiag/report.json" should have schema "builddiag.report.v1"
    And the report verdict should be "error"
    And the report should include finding "tool.runtime" "runtime_error" "error"

  Scenario: Artifacts directory overrides explicit out path
    Given the workspace has MSRV "1.75.0" in workspace package
    And the workspace has a pinned toolchain "1.75.0"
    And the workspace has a checksums file
    And artifacts are written to "ci-artifacts"
    When I run builddiag check with --out "custom-report.json"
    Then the exit code should be 0
    And the sensor report should exist at the canonical path
    And the file "ci-artifacts/report.json" should exist
    And the file "custom-report.json" should not exist
