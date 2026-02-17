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
