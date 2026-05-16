//! Integration tests for the `builddiag explain` command.
//!
//! These tests validate CLI behavior across the four code paths the
//! `explain` subcommand exposes: explain-by-check-id, explain-by-code,
//! the legacy `CHECK_DOCS` fallback, and the two unknown-input error
//! branches.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper to get the builddiag command.
#[allow(deprecated)]
fn get_builddiag_cmd() -> Command {
    Command::cargo_bin("builddiag").unwrap()
}

// =============================================================================
// Explain by check ID
// =============================================================================

/// Test: `builddiag explain <check.id>` prints the per-code entries with
/// the standard `What it means` / `Why it matters` / `How to fix` sections.
#[test]
fn explain_check_id_prints_sections_and_codes() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("rust.msrv_defined");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Check: rust.msrv_defined"))
        .stdout(predicate::str::contains("What it means:"))
        .stdout(predicate::str::contains("Why it matters:"))
        .stdout(predicate::str::contains("How to fix:"))
        .stdout(predicate::str::contains("Code: missing_msrv"));
}

/// Test: `builddiag explain <check.id>` includes the equals-sign underline
/// matching the header length (7 + len("rust.msrv_defined") characters).
#[test]
fn explain_check_id_includes_header_underline() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("rust.msrv_defined");

    // "Check: rust.msrv_defined" is 24 characters wide; the underline is 24 `=`.
    let expected_underline = "=".repeat(7 + "rust.msrv_defined".len());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(expected_underline));
}

// =============================================================================
// Explain by finding code
// =============================================================================

/// Test: `builddiag explain <code>` (no dot) prints the "Check: ... / Code: ..."
/// header for the matching check.
#[test]
fn explain_finding_code_prints_check_and_code_header() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("missing_msrv");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Check: rust.msrv_defined / Code: missing_msrv",
        ))
        .stdout(predicate::str::contains("What it means:"))
        .stdout(predicate::str::contains("How to fix:"));
}

// =============================================================================
// Legacy CHECK_DOCS fallback
// =============================================================================

/// Test: `builddiag explain deps.wildcard_version` (a check whose ID is only
/// in `CHECK_DOCS`, not `explain_check_all_codes`) exercises the
/// `write_legacy_doc` fallback path.
#[test]
fn explain_check_id_legacy_fallback() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("deps.wildcard_version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No Wildcard Versions"))
        .stdout(predicate::str::contains("Help:"))
        .stdout(predicate::str::contains("Finding codes:"))
        .stdout(predicate::str::contains("- wildcard_version"));
}

/// Test: a bare code (no dot) that is only present in legacy `CHECK_DOCS`
/// also routes through `write_legacy_doc`.
#[test]
fn explain_finding_code_legacy_fallback() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("wildcard_version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No Wildcard Versions"))
        .stdout(predicate::str::contains("Finding codes:"));
}

// =============================================================================
// Unknown input
// =============================================================================

/// Test: `builddiag explain <unknown.id>` (with dot) exits 1 and lists
/// available checks on stderr.
#[test]
fn explain_unknown_check_id_fails_with_listing() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("does.not.exist");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Unknown check: 'does.not.exist'"))
        .stderr(predicate::str::contains("Available checks:"))
        .stderr(predicate::str::contains("rust.msrv_defined"))
        .stderr(predicate::str::contains("builddiag list-checks"));
}

/// Test: `builddiag explain <unknown-code>` (no dot) exits 1 and points the
/// user at `list-checks` via stderr.
#[test]
fn explain_unknown_finding_code_fails_with_pointer() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("definitely_not_a_real_code");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "Unknown check or finding code: 'definitely_not_a_real_code'",
        ))
        .stderr(predicate::str::contains("builddiag list-checks"));
}

// =============================================================================
// Multi-code checks
// =============================================================================

/// Test: `builddiag explain <multi-code check>` prints the dashed separator
/// line between consecutive entries.
#[test]
fn explain_multi_code_check_includes_dash_separator() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain").arg("tools.checksums_format");

    // The implementation writes a 60-dash separator between entries.
    let separator = "-".repeat(60);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(separator))
        .stdout(predicate::str::contains("Code: missing_path"))
        .stdout(predicate::str::contains("Code: duplicate_path"));
}

// =============================================================================
// Missing argument
// =============================================================================

/// Test: `builddiag explain` (no argument) fails with a clap usage error.
#[test]
fn explain_missing_argument_fails() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("explain");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("check_or_code").or(predicate::str::contains("required")));
}
