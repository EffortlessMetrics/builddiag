//! Integration tests for the `builddiag list-checks` command.
//!
//! These tests cover the default table format, the JSON format, and the
//! three `--profile` filters (oss, team, strict) so that every branch in
//! `list_checks_table` / `list_checks_json` / `severity_str` is exercised.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

/// Helper to get the builddiag command.
#[allow(deprecated)]
fn get_builddiag_cmd() -> Command {
    Command::cargo_bin("builddiag").unwrap()
}

/// Run `builddiag list-checks` with the given args and capture stdout.
fn run_list_checks(args: &[&str]) -> String {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(output).expect("list-checks stdout should be UTF-8")
}

// =============================================================================
// Default (table) format
// =============================================================================

/// Test: `builddiag list-checks` with no args prints the table header and
/// includes a known check ID like `rust.msrv_defined`.
#[test]
fn list_checks_default_table_has_header_and_known_check() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available checks:"))
        .stdout(predicate::str::contains("CHECK ID"))
        .stdout(predicate::str::contains("NAME"))
        .stdout(predicate::str::contains("OSS"))
        .stdout(predicate::str::contains("TEAM"))
        .stdout(predicate::str::contains("STRICT"))
        .stdout(predicate::str::contains("rust.msrv_defined"))
        .stdout(predicate::str::contains(
            "Use 'builddiag explain <check-id>' for detailed documentation.",
        ));
}

/// Test: The default table includes severities from every profile (the
/// three-column variant), so output covers `info`, `warn`, and `error`
/// strings produced by `severity_str` across all three profiles.
#[test]
fn list_checks_default_table_contains_all_severities() {
    let out = run_list_checks(&[]);
    // `rust.msrv_defined` is warn/warn/error across oss/team/strict.
    assert!(out.contains("warn"), "expected warn severity in output");
    assert!(out.contains("error"), "expected error severity in output");
    // `workspace.member_ordering` is info/info/error.
    assert!(out.contains("info"), "expected info severity in output");
    // `tools.checksums_file_exists` is skip in oss; the multi-profile table
    // still includes it so `skip` should appear.
    assert!(
        out.contains("skip"),
        "expected skip severity in multi-profile table"
    );
}

// =============================================================================
// JSON format
// =============================================================================

/// Test: `builddiag list-checks --format json` returns a JSON array of
/// CheckInfo objects with the documented shape.
#[test]
fn list_checks_json_shape_matches_spec() {
    let stdout = run_list_checks(&["--format", "json"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be JSON");
    let array = parsed.as_array().expect("top-level JSON must be an array");
    assert!(!array.is_empty(), "list-checks JSON must not be empty");

    // Every entry must have id, name, description, codes, profiles
    // with `oss`, `team`, and `strict` sub-objects, each having
    // `enabled: bool` and `severity` (null or string).
    for entry in array {
        let obj = entry.as_object().expect("each entry must be an object");
        assert!(obj.contains_key("id"), "entry missing id");
        assert!(obj.contains_key("name"), "entry missing name");
        assert!(obj.contains_key("description"), "entry missing description");
        assert!(obj.contains_key("codes"), "entry missing codes");

        let profiles = obj
            .get("profiles")
            .and_then(|v| v.as_object())
            .expect("entry missing profiles object");
        for key in ["oss", "team", "strict"] {
            let state = profiles
                .get(key)
                .and_then(|v| v.as_object())
                .unwrap_or_else(|| panic!("profile entry missing key {key}"));
            assert!(
                state.contains_key("enabled"),
                "profile {key} missing enabled"
            );
            assert!(
                state.contains_key("severity"),
                "profile {key} missing severity"
            );
        }
    }

    // Spot-check: rust.msrv_defined is present and enabled in all three profiles
    let msrv = array
        .iter()
        .find(|v| v.get("id").and_then(|i| i.as_str()) == Some("rust.msrv_defined"))
        .expect("rust.msrv_defined must appear in JSON output");
    let profiles = msrv.get("profiles").unwrap().as_object().unwrap();
    for key in ["oss", "team", "strict"] {
        assert_eq!(
            profiles.get(key).unwrap().get("enabled").unwrap(),
            &Value::Bool(true)
        );
    }
}

// =============================================================================
// Profile filters - table
// =============================================================================

/// Test: `--profile oss` shows the OSS profile and excludes `skip` checks
/// like `tools.checksums_file_exists`.
#[test]
fn list_checks_profile_oss_filters_skipped() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks").arg("--profile").arg("oss");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available checks:"))
        .stdout(predicate::str::contains("SEVERITY"))
        .stdout(predicate::str::contains("rust.msrv_defined"))
        // `tools.checksums_file_exists` is `skip` under OSS - must not show.
        .stdout(predicate::str::contains("tools.checksums_file_exists").not());
}

/// Test: `--profile team` exercises the Team branch of the filter and
/// includes checks that are warn/error in team but skip in oss.
#[test]
fn list_checks_profile_team_branch_executes() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks").arg("--profile").arg("team");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Available checks:"))
        // Under team, this check is enabled with severity warn.
        .stdout(predicate::str::contains("tools.checksums_file_exists"))
        .stdout(predicate::str::contains("rust.msrv_defined"));
}

/// Test: `--profile strict` shows every check at `error` severity.
#[test]
fn list_checks_profile_strict_shows_all_checks() {
    let out = run_list_checks(&["--profile", "strict"]);
    // Under strict, all builtin checks are enabled with severity error.
    assert!(out.contains("rust.msrv_defined"));
    assert!(out.contains("tools.checksums_file_exists"));
    assert!(out.contains("deps.security_advisory"));
    assert!(out.contains("error"));
    // Strict has no info or warn severities in builtin checks.
    // We don't assert absence here because the literal substring "warn" might
    // collide with anything else; just confirm the strict profile severity is
    // visible.
}

/// Test: Profile-filtered table uses the single-severity column header
/// instead of the three-profile header.
#[test]
fn list_checks_profile_table_has_single_severity_column() {
    let out = run_list_checks(&["--profile", "oss"]);
    assert!(out.contains("SEVERITY"));
    // The multi-profile header has OSS/TEAM/STRICT columns; in the
    // profile-filtered mode they are not present.
    assert!(!out.contains("OSS   TEAM"));
}

// =============================================================================
// Profile filters - JSON
// =============================================================================

/// Test: `--profile strict --format json` only includes checks enabled
/// under the strict profile (every builtin) and the JSON parses cleanly.
#[test]
fn list_checks_profile_strict_json_includes_only_enabled() {
    let stdout = run_list_checks(&["--profile", "strict", "--format", "json"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be JSON");
    let array = parsed.as_array().expect("top-level JSON must be an array");
    assert!(!array.is_empty(), "strict profile must include checks");

    // Every entry must be enabled under the strict profile (since we filter
    // out Skip entries server-side).
    for entry in array {
        let strict_state = entry
            .pointer("/profiles/strict")
            .and_then(|v| v.as_object())
            .expect("entry must have profiles.strict");
        assert_eq!(
            strict_state.get("enabled"),
            Some(&Value::Bool(true)),
            "filter should exclude disabled entries"
        );
    }
}

/// Test: `--profile oss --format json` excludes the OSS-skipped checks.
#[test]
fn list_checks_profile_oss_json_excludes_skipped() {
    let stdout = run_list_checks(&["--profile", "oss", "--format", "json"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be JSON");
    let array = parsed.as_array().unwrap();

    let ids: Vec<&str> = array
        .iter()
        .filter_map(|v| v.get("id").and_then(|i| i.as_str()))
        .collect();

    assert!(ids.contains(&"rust.msrv_defined"));
    // OSS skips all `tools.*` checksum checks and `deps.security_advisory`.
    assert!(!ids.contains(&"tools.checksums_file_exists"));
    assert!(!ids.contains(&"deps.security_advisory"));
}

/// Test: `--profile team --format json` includes the Team-enabled subset.
#[test]
fn list_checks_profile_team_json_branch_executes() {
    let stdout = run_list_checks(&["--profile", "team", "--format", "json"]);
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout must be JSON");
    let array = parsed.as_array().unwrap();

    // Each entry should be Team-enabled.
    for entry in array {
        let team_state = entry
            .pointer("/profiles/team")
            .and_then(|v| v.as_object())
            .expect("entry must have profiles.team");
        assert_eq!(
            team_state.get("enabled"),
            Some(&Value::Bool(true)),
            "team filter should exclude disabled entries"
        );
    }

    let ids: Vec<&str> = array
        .iter()
        .filter_map(|v| v.get("id").and_then(|i| i.as_str()))
        .collect();
    // Team enables tools.checksums_file_exists with warn severity.
    assert!(ids.contains(&"tools.checksums_file_exists"));
}

// =============================================================================
// Invalid arguments
// =============================================================================

/// Test: An unknown `--format` value is rejected by clap.
#[test]
fn list_checks_invalid_format_rejected() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks").arg("--format").arg("yaml");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("yaml").or(predicate::str::contains("invalid value")));
}

/// Test: An unknown `--profile` value is rejected by clap.
#[test]
fn list_checks_invalid_profile_rejected() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("list-checks").arg("--profile").arg("nope");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("nope").or(predicate::str::contains("invalid value")));
}
