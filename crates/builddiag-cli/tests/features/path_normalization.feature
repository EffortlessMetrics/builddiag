@paths
Feature: Path Normalization
  I want report location paths to be repo-relative with forward slashes

  Scenario: Findings use forward slashes in location paths
    Given a Rust workspace
    And the workspace has MSRV "1.75.0" in workspace package
    And the crate is missing publish metadata
    When I run builddiag check
    Then the report findings should use forward slashes
