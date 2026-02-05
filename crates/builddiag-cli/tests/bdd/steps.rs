//! Step definitions for cucumber tests.
//!
//! This module implements Given/When/Then steps for BDD scenarios.

use cucumber::{given, then, when};
use serde_json::Value;
use std::path::PathBuf;

use super::helpers::{materialize_workspace, run_builddiag_check};
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

#[given(expr = "the config file is named {string}")]
fn given_config_file_name(world: &mut BuilddiagWorld, name: String) {
    world.config_path = Some(name);
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
    let stdout = world.stdout();
    assert!(
        stdout.contains(&expected),
        "Expected stdout to contain '{}' but got:\n{}",
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
    assert!(
        report_path.exists(),
        "Expected report to exist at canonical path {:?}",
        report_path
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
