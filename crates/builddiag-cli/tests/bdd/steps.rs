//! Step definitions for cucumber tests.
//!
//! This module implements Given/When/Then steps for BDD scenarios.

use builddiag_output_contract::{
    load_and_validate_builddiag_report, load_and_validate_sensor_report, load_sensor_report,
};
use builddiag_types::SensorReport;
use cucumber::{given, then, when};
use serde_json::Value;
use std::path::PathBuf;

use super::helpers::{materialize_workspace, run_builddiag_check, run_builddiag_list_checks};
use super::world::{BuilddiagWorld, MsrvConfig, MsrvLocation, ToolchainConfig};

// =============================================================================
// Given steps - Set up preconditions
// =============================================================================

#[given("a Rust workspace")]
fn given_rust_workspace(world: &mut BuilddiagWorld) {
    world.has_workspace = true;
}

#[given("the workspace has no MSRV defined")]
fn given_no_msrv(world: &mut BuilddiagWorld) {
    world.msrv = Some(MsrvConfig {
        version: String::new(),
        location: MsrvLocation::None,
    });
}

#[given(expr = "the workspace has MSRV {string} in workspace package")]
fn given_msrv_in_workspace(world: &mut BuilddiagWorld, version: String) {
    world.msrv = Some(MsrvConfig {
        version,
        location: MsrvLocation::WorkspacePackage,
    });
}

#[given(expr = "the workspace has MSRV {string} only in crate")]
fn given_msrv_in_crate_only(world: &mut BuilddiagWorld, version: String) {
    world.msrv = Some(MsrvConfig {
        version,
        location: MsrvLocation::CrateOnly,
    });
}

#[given(expr = "the workspace has a pinned toolchain {string}")]
fn given_pinned_toolchain(world: &mut BuilddiagWorld, channel: String) {
    world.toolchain = Some(ToolchainConfig { channel });
}

#[given(expr = "the workspace has an unpinned toolchain {string}")]
fn given_unpinned_toolchain(world: &mut BuilddiagWorld, channel: String) {
    world.toolchain = Some(ToolchainConfig { channel });
}

#[given("the workspace has no rust-toolchain.toml")]
fn given_no_toolchain(world: &mut BuilddiagWorld) {
    world.toolchain = None;
}

#[given("the workspace has a checksums file")]
fn given_checksums_file(world: &mut BuilddiagWorld) {
    world.has_checksums = true;
}

#[given("the workspace has no checksums file")]
fn given_no_checksums_file(world: &mut BuilddiagWorld) {
    world.has_checksums = false;
}

#[given(expr = "the config has msrv source {string}")]
fn given_msrv_source_config(world: &mut BuilddiagWorld, source: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[policy.msrv]
source = "{}"
"#,
        source
    ));
}

#[given(expr = "the config has toolchain relation_to_msrv {string}")]
fn given_toolchain_relation_config(world: &mut BuilddiagWorld, relation: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[policy.toolchain]
relation_to_msrv = "{}"
"#,
        relation
    ));
}

#[given(expr = "the config has toolchain require_pinned {string}")]
fn given_toolchain_require_pinned_config(world: &mut BuilddiagWorld, value: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[policy.toolchain]
require_pinned = {}
"#,
        value
    ));
}

#[given(expr = "the config has checksums require_file {string}")]
fn given_checksums_require_file_config(world: &mut BuilddiagWorld, value: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[policy.checksums]
require_file = {}
"#,
        value
    ));
}

#[given(expr = "the config has fail_on {string}")]
fn given_fail_on_config(world: &mut BuilddiagWorld, value: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[defaults]
fail_on = "{}"
"#,
        value
    ));
}

#[given(expr = "the config has out_dir {string}")]
fn given_out_dir_config(world: &mut BuilddiagWorld, value: String) {
    let content = world.config_content.get_or_insert_with(String::new);
    content.push_str(&format!(
        r#"[defaults]
out_dir = "{}"
"#,
        value
    ));
    world.out_dir_override = Some(value);
}

#[given(expr = "the output format is {string}")]
fn given_output_format(world: &mut BuilddiagWorld, value: String) {
    world.extra_args.push("--format".to_string());
    world.extra_args.push(value);
}

#[given(expr = "the check mode is {string}")]
fn given_check_mode(world: &mut BuilddiagWorld, value: String) {
    world.extra_args.push("--mode".to_string());
    world.extra_args.push(value);
}

#[given(expr = "artifacts are written to {string}")]
fn given_artifacts_dir(world: &mut BuilddiagWorld, value: String) {
    world.extra_args.push("--artifacts-dir".to_string());
    world.extra_args.push(value.clone());
    world.artifacts_dir_override = Some(value);
}

#[given(expr = "the config file is named {string}")]
fn given_config_file_name(world: &mut BuilddiagWorld, name: String) {
    world.config_path = Some(name);
}

#[given("the config file is invalid")]
fn given_invalid_config(world: &mut BuilddiagWorld) {
    world.config_content = Some("[defaults".to_string());
}

#[given(expr = "the workspace has crates {string}")]
fn given_additional_crates(world: &mut BuilddiagWorld, crates: String) {
    for crate_name in crates.split(',') {
        let name = crate_name.trim();
        if name != "a" {
            world.additional_crates.push(name.to_string());
        }
    }
}

#[given(expr = "the workspace edition is {string}")]
fn given_workspace_edition(world: &mut BuilddiagWorld, edition: String) {
    world.workspace_edition = Some(edition);
}

#[given("the crate is missing publish metadata")]
fn given_crate_missing_publish_metadata(world: &mut BuilddiagWorld) {
    world.custom_files.insert(
        "crates/a/Cargo.toml".to_string(),
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#
        .to_string(),
    );
}

#[given("the crate is publish-disabled and missing publish metadata")]
fn given_crate_publish_disabled_missing_publish_metadata(world: &mut BuilddiagWorld) {
    world.custom_files.insert(
        "crates/a/Cargo.toml".to_string(),
        r#"[package]
name = "a"
version = "0.1.0"
publish = false
edition.workspace = true
rust-version.workspace = true
"#
        .to_string(),
    );
}

#[given("the workspace has conflicting serde versions across members")]
fn given_workspace_has_conflicting_serde_versions(world: &mut BuilddiagWorld) {
    if !world.additional_crates.iter().any(|c| c == "b") {
        world.additional_crates.push("b".to_string());
    }

    world.custom_files.insert(
        "crates/a/Cargo.toml".to_string(),
        r#"[package]
name = "a"
version = "0.1.0"
description = "crate a"
license = "MIT"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = "1.0.188"
"#
        .to_string(),
    );

    world.custom_files.insert(
        "crates/b/Cargo.toml".to_string(),
        r#"[package]
name = "b"
version = "0.1.0"
description = "crate b"
license = "MIT"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = "1.0.200"
"#
        .to_string(),
    );
}

#[given("the crate has a binary target")]
fn given_crate_has_binary_target(world: &mut BuilddiagWorld) {
    world.custom_files.insert(
        "crates/a/src/main.rs".to_string(),
        "fn main() {}\n".to_string(),
    );
}

#[given("the workspace has a Cargo.lock file")]
fn given_workspace_has_lockfile(world: &mut BuilddiagWorld) {
    world
        .custom_files
        .insert("Cargo.lock".to_string(), String::new());
}

// =============================================================================
// When steps - Perform actions
// =============================================================================

#[when("I run builddiag check")]
fn when_run_check(world: &mut BuilddiagWorld) {
    materialize_workspace(world);
    run_builddiag_check(world);
}

#[when(expr = "I run builddiag check with profile {string}")]
fn when_run_check_with_profile(world: &mut BuilddiagWorld, profile: String) {
    world.profile = Some(profile);
    materialize_workspace(world);
    run_builddiag_check(world);
}

#[when(expr = "I run builddiag check with --{word} {string}")]
fn when_run_check_with_flag(world: &mut BuilddiagWorld, flag: String, value: String) {
    if flag == "out" {
        world.explicit_out = Some(value.clone());
    } else if flag == "artifacts-dir" {
        world.artifacts_dir_override = Some(value.clone());
    }
    world.extra_args.push(format!("--{}", flag));
    world.extra_args.push(value);
    materialize_workspace(world);
    run_builddiag_check(world);
}

#[when(expr = "I run builddiag check with --{word}")]
fn when_run_check_with_bool_flag(world: &mut BuilddiagWorld, flag: String) {
    world.extra_args.push(format!("--{}", flag));
    materialize_workspace(world);
    run_builddiag_check(world);
}

#[when("I run builddiag list-checks")]
fn when_run_list_checks(world: &mut BuilddiagWorld) {
    materialize_workspace(world);
    run_builddiag_list_checks(world);
}

#[when(expr = "I run builddiag list-checks with profile {string}")]
fn when_run_list_checks_with_profile(world: &mut BuilddiagWorld, profile: String) {
    world.extra_args.push("--profile".to_string());
    world.extra_args.push(profile);
    materialize_workspace(world);
    run_builddiag_list_checks(world);
}

#[when(expr = "I run builddiag list-checks with format {string}")]
fn when_run_list_checks_with_format(world: &mut BuilddiagWorld, format: String) {
    world.extra_args.push("--format".to_string());
    world.extra_args.push(format);
    materialize_workspace(world);
    run_builddiag_list_checks(world);
}

// =============================================================================
// Then steps - Verify outcomes
// =============================================================================

#[then(expr = "the exit code should be {int}")]
fn then_exit_code(world: &mut BuilddiagWorld, expected: i32) {
    let actual = world.exit_code();
    assert_eq!(
        actual,
        expected,
        "Expected exit code {} but got {}.\nstdout: {}\nstderr: {}",
        expected,
        actual,
        world.stdout(),
        world.stderr()
    );
}

#[then("the check should pass")]
fn then_check_passes(world: &mut BuilddiagWorld) {
    let code = world.exit_code();
    assert_eq!(
        code,
        0,
        "Expected check to pass (exit 0) but got exit code {}.\nstdout: {}\nstderr: {}",
        code,
        world.stdout(),
        world.stderr()
    );
}

#[then("the check should fail")]
fn then_check_fails(world: &mut BuilddiagWorld) {
    let code = world.exit_code();
    assert_eq!(
        code,
        2,
        "Expected check to fail (exit 2) but got exit code {}.\nstdout: {}\nstderr: {}",
        code,
        world.stdout(),
        world.stderr()
    );
}

#[then("the check should warn")]
fn then_check_warns(world: &mut BuilddiagWorld) {
    let code = world.exit_code();
    assert!(
        code == 0 || code == 1,
        "Expected check to warn but got exit code {}.\nstdout: {}\nstderr: {}",
        code,
        world.stdout(),
        world.stderr()
    );
}

#[then(expr = "stdout should contain {string}")]
fn then_stdout_contains(world: &mut BuilddiagWorld, expected: String) {
    let expected = expected.replace("\\\"", "\"");
    let stdout = world.stdout();
    assert!(
        stdout.contains(&expected),
        "Expected stdout to contain '{}' but got:\n{}",
        expected,
        stdout
    );
}

#[then(expr = "stdout should not contain {string}")]
fn then_stdout_not_contains(world: &mut BuilddiagWorld, expected: String) {
    let expected = expected.replace("\\\"", "\"");
    let stdout = world.stdout();
    assert!(
        !stdout.contains(&expected),
        "Expected stdout to not contain '{}' but got:\n{}",
        expected,
        stdout
    );
}

#[then(expr = "stderr should contain {string}")]
fn then_stderr_contains(world: &mut BuilddiagWorld, expected: String) {
    let stderr = world.stderr();
    assert!(
        stderr.contains(&expected),
        "Expected stderr to contain '{}' but got:\n{}",
        expected,
        stderr
    );
}

#[then(expr = "the file {string} should exist")]
fn then_file_exists(world: &mut BuilddiagWorld, path: String) {
    let full_path = world.workspace_path().join(&path);
    assert!(
        full_path.exists(),
        "Expected file '{}' to exist at {:?}",
        path,
        full_path
    );
}

#[then(expr = "the file {string} should not exist")]
fn then_file_not_exists(world: &mut BuilddiagWorld, path: String) {
    let full_path = world.workspace_path().join(&path);
    assert!(
        !full_path.exists(),
        "Expected file '{}' to not exist at {:?}",
        path,
        full_path
    );
}

#[then("the report should exist at the canonical path")]
fn then_report_exists_at_canonical_path(world: &mut BuilddiagWorld) {
    let report_path = canonical_report_path(world);
    load_and_validate_builddiag_report(&report_path)
        .unwrap_or_else(|err| panic!("failed to validate report at {:?}: {err}", report_path));
}

#[then("the sensor report should exist at the canonical path")]
fn then_sensor_report_exists_at_canonical_path(world: &mut BuilddiagWorld) {
    let report_path = canonical_report_path(world);
    load_and_validate_sensor_report(&report_path).unwrap_or_else(|err| {
        panic!(
            "failed to validate sensor report at {:?}: {err}",
            report_path
        )
    });
}

#[then(expr = "the sensor report verdict status should be {string}")]
fn then_sensor_report_verdict_status(world: &mut BuilddiagWorld, expected_status: String) {
    let report = read_sensor_report(world);
    let actual = match report.verdict.status {
        builddiag_types::VerdictStatus::Pass => "pass",
        builddiag_types::VerdictStatus::Warn => "warn",
        builddiag_types::VerdictStatus::Fail => "fail",
        builddiag_types::VerdictStatus::Skip => "skip",
    };
    assert_eq!(
        actual, expected_status,
        "Expected sensor verdict status '{}' but got '{}'",
        expected_status, actual
    );
}

#[then(expr = "the sensor report verdict should include reason {string}")]
fn then_sensor_report_verdict_reason(world: &mut BuilddiagWorld, expected_reason: String) {
    let report = read_sensor_report(world);
    assert!(
        report.verdict.reasons.iter().any(|r| r == &expected_reason),
        "Expected sensor verdict reasons {:?} to include '{}'",
        report.verdict.reasons,
        expected_reason
    );
}

#[then(expr = "the sensor report verdict data should include {string} in {string}")]
fn then_sensor_report_verdict_data_contains(
    world: &mut BuilddiagWorld,
    expected_value: String,
    key: String,
) {
    let report = read_sensor_report(world);
    let data = report
        .verdict
        .data
        .as_ref()
        .expect("sensor verdict data should be present");
    let values = data
        .get(&key)
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "sensor verdict data should contain an array at key '{}'",
                key
            )
        });
    let found = values
        .iter()
        .any(|v| v.as_str() == Some(expected_value.as_str()));
    assert!(
        found,
        "Expected sensor verdict data '{}' to include '{}', got {:?}",
        key, expected_value, values
    );
}

#[then(expr = "the sensor report should include capabilities {string}")]
fn then_sensor_report_has_capabilities(world: &mut BuilddiagWorld, names: String) {
    let report = read_sensor_report(world);
    let run = report
        .run
        .as_ref()
        .expect("sensor run should be present with capabilities");
    for name in names.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        assert!(
            run.capabilities.contains_key(name),
            "Expected capability '{}' to be present, got keys {:?}",
            name,
            run.capabilities.keys().collect::<Vec<_>>()
        );
    }
}

#[then(expr = "the sensor capability {string} should be {string}")]
fn then_sensor_report_capability_status(
    world: &mut BuilddiagWorld,
    capability: String,
    expected_status: String,
) {
    let report = read_sensor_report(world);
    let run = report
        .run
        .as_ref()
        .expect("sensor run should be present with capabilities");
    let actual = run
        .capabilities
        .get(&capability)
        .unwrap_or_else(|| panic!("Expected capability '{}' to exist", capability))
        .status;
    let actual_status = match actual {
        builddiag_types::CapabilityStatus::Available => "available",
        builddiag_types::CapabilityStatus::Unavailable => "unavailable",
        builddiag_types::CapabilityStatus::Skipped => "skipped",
    };
    assert_eq!(
        actual_status, expected_status,
        "Expected capability '{}' to be '{}' but got '{}'",
        capability, expected_status, actual_status
    );
}

#[then(expr = "the sensor report should include artifact {string} at {string}")]
fn then_sensor_report_has_artifact(world: &mut BuilddiagWorld, name: String, path: String) {
    let report = read_sensor_report(world);
    let found = report
        .artifacts
        .iter()
        .any(|artifact| artifact.name == name && artifact.path == path);
    assert!(
        found,
        "Expected sensor artifact '{}' at '{}' to be present",
        name, path
    );
}

#[then(expr = "the file {string} should have schema {string}")]
fn then_file_has_schema(world: &mut BuilddiagWorld, path: String, expected_schema: String) {
    let full_path = world.workspace_path().join(&path);
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|_| panic!("failed to read file at {:?}", full_path));
    let value: Value = serde_json::from_str(&content)
        .unwrap_or_else(|_| panic!("failed to parse JSON at {:?}", full_path));
    let actual = value
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("schema not found in {:?}", full_path));
    assert_eq!(
        actual, expected_schema,
        "Expected schema '{}' at {:?}, got '{}'",
        expected_schema, full_path, actual
    );
}

#[then(expr = "the report verdict should be {string}")]
fn then_report_verdict_is(world: &mut BuilddiagWorld, expected_verdict: String) {
    let report = read_report(world);
    let verdict = report["verdict"]
        .as_str()
        .expect("verdict not found in report");
    assert_eq!(
        verdict, expected_verdict,
        "Expected verdict '{}' but got '{}'",
        expected_verdict, verdict
    );
}

#[then(expr = "the report should have verdict {string}")]
fn then_report_verdict(world: &mut BuilddiagWorld, expected_verdict: String) {
    let report = read_report(world);
    let verdict = report["verdict"]
        .as_str()
        .expect("verdict not found in report");
    assert_eq!(
        verdict, expected_verdict,
        "Expected verdict '{}' but got '{}'",
        expected_verdict, verdict
    );
}

#[then(expr = "the report should include finding {string} {string} {string}")]
fn then_report_includes_finding(
    world: &mut BuilddiagWorld,
    check_id: String,
    code: String,
    severity: String,
) {
    let report = read_report(world);
    let findings = report["findings"]
        .as_array()
        .expect("findings not found in report");
    let found = findings.iter().any(|f| {
        f["check_id"].as_str() == Some(check_id.as_str())
            && f["code"].as_str() == Some(code.as_str())
            && f["severity"].as_str() == Some(severity.as_str())
    });
    assert!(
        found,
        "Expected finding ({}, {}, {}) not found in report",
        check_id, code, severity
    );
}

#[then("the report findings should use forward slashes")]
fn then_report_findings_use_forward_slashes(world: &mut BuilddiagWorld) {
    let report = read_report(world);
    let findings = report["findings"]
        .as_array()
        .expect("findings not found in report");

    let mut saw_location = false;
    for finding in findings {
        if let Some(location) = finding.get("location") {
            let path = location
                .get("path")
                .and_then(|v| v.as_str())
                .expect("location.path should be a string");
            saw_location = true;
            assert!(
                !path.contains('\\'),
                "Expected location path to use forward slashes, got: {}",
                path
            );
        }
    }

    assert!(
        saw_location,
        "Expected at least one finding with a location path"
    );
}

#[then(expr = "the report should not include finding {string} {string}")]
fn then_report_does_not_include_finding(
    world: &mut BuilddiagWorld,
    check_id: String,
    code: String,
) {
    let report = read_report(world);
    let findings = report["findings"]
        .as_array()
        .expect("findings not found in report");
    let found = findings.iter().any(|f| {
        f["check_id"].as_str() == Some(check_id.as_str())
            && f["code"].as_str() == Some(code.as_str())
    });
    assert!(
        !found,
        "Expected finding ({}, {}) to be absent in report",
        check_id, code
    );
}

#[then(expr = "the report should not include any {string} findings")]
fn then_report_has_no_findings_with_severity(world: &mut BuilddiagWorld, severity: String) {
    let report = read_report(world);
    let findings = report["findings"]
        .as_array()
        .expect("findings not found in report");
    let found = findings
        .iter()
        .any(|f| f["severity"].as_str() == Some(severity.as_str()));
    assert!(
        !found,
        "Expected no '{}' findings but found at least one",
        severity
    );
}

fn canonical_report_path(world: &BuilddiagWorld) -> PathBuf {
    if let Some(ref artifacts_dir) = world.artifacts_dir_override {
        return world
            .workspace_path()
            .join(artifacts_dir)
            .join("report.json");
    }

    if let Some(ref out) = world.explicit_out {
        let out_path = PathBuf::from(out);
        if out_path.is_absolute() {
            return out_path;
        }
        return world.workspace_path().join(out_path);
    }

    let out_dir = world
        .out_dir_override
        .as_deref()
        .unwrap_or("artifacts/builddiag");
    world.workspace_path().join(out_dir).join("report.json")
}

fn read_report(world: &BuilddiagWorld) -> Value {
    let report_path = canonical_report_path(world);
    let content = std::fs::read_to_string(&report_path)
        .unwrap_or_else(|_| panic!("failed to read report.json at {:?}", report_path));
    serde_json::from_str(&content)
        .unwrap_or_else(|_| panic!("failed to parse report.json at {:?}", report_path))
}

fn read_sensor_report(world: &BuilddiagWorld) -> SensorReport {
    let report_path = canonical_report_path(world);
    load_sensor_report(&report_path)
        .unwrap_or_else(|_| panic!("failed to parse sensor report at {:?}", report_path))
}
