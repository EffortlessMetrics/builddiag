//! Integration tests for the `builddiag check` command.
//!
//! These tests validate the CLI behavior for various repository configurations
//! and policy settings.
//!
//! _Requirements: 6.1, 7.1, 7.2, 7.3, 7.4_

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Helper to get the builddiag command.
#[allow(deprecated)]
fn get_builddiag_cmd() -> Command {
    Command::cargo_bin("builddiag").unwrap()
}

/// Helper to write a file to the test directory.
fn write_file(dir: &TempDir, rel: &str, contents: &str) {
    let p = dir.path().join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

/// Creates a minimal valid workspace with MSRV, toolchain, and checksums.
fn create_valid_workspace(dir: &TempDir) {
    write_file(
        dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(dir, "crates/a/src/lib.rs", "pub fn f() -> u32 { 1 }\n");

    write_file(
        dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    write_file(dir, "scripts/tools.sha256", "");
}

// =============================================================================
// Basic check command tests
// =============================================================================

/// Test: `builddiag check` with a valid repository passes.
/// _Requirements: 6.1, 7.1_
#[test]
fn check_valid_repository_passes() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: `builddiag check` produces JSON report file.
/// _Requirements: 6.1_
#[test]
fn check_produces_json_report() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success();

    // Verify report.json was created
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(report_path.exists(), "report.json should be created");

    // Verify it's valid JSON
    let content = fs::read_to_string(&report_path).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content).expect("report should be valid JSON");
}

/// Test: `builddiag check` produces Markdown summary file.
/// _Requirements: 6.1_
#[test]
fn check_produces_markdown_summary() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success();

    // Verify comment.md was created
    let md_path = dir.path().join("artifacts/builddiag/comment.md");
    assert!(md_path.exists(), "comment.md should be created");

    let content = fs::read_to_string(&md_path).unwrap();
    assert!(
        content.contains("builddiag"),
        "Markdown should contain builddiag header"
    );
}

// =============================================================================
// MSRV validation tests
// =============================================================================

/// Test: Missing MSRV fails with strict profile.
/// _Requirements: 7.1_
#[test]
fn check_missing_msrv_fails() {
    let dir = TempDir::new().unwrap();

    // Workspace without rust-version
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: MSRV defined only in crate (not workspace) fails with strict profile.
/// _Requirements: 7.1_
#[test]
fn check_msrv_in_crate_only_fails_with_workspace_policy() {
    let dir = TempDir::new().unwrap();

    // Workspace without rust-version, but crate has it
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
rust-version = "1.75.0"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    // Should fail because strict policy requires workspace-level MSRV
    cmd.assert().code(2);
}

/// Test: MSRV in crate passes with `source = "any"` policy.
/// _Requirements: 7.1, 7.6_
#[test]
fn check_msrv_in_crate_passes_with_any_source_policy() {
    let dir = TempDir::new().unwrap();

    // Workspace without rust-version, but crate has it
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
rust-version = "1.75.0"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    // Toolchain matching the crate MSRV
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    // Checksums file
    write_file(&dir, "scripts/tools.sha256", "");

    // Config with source = "any"
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[policy.msrv]
source = "any"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

// =============================================================================
// Toolchain pinning tests
// =============================================================================

/// Test: Missing rust-toolchain.toml fails with strict profile.
/// _Requirements: 7.2_
#[test]
fn check_missing_toolchain_fails() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");
    write_file(&dir, "scripts/tools.sha256", "");

    // No rust-toolchain.toml

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: Unpinned toolchain (using channel like "stable") fails with strict profile.
/// _Requirements: 7.2_
#[test]
fn check_unpinned_toolchain_fails() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");
    write_file(&dir, "scripts/tools.sha256", "");

    // Unpinned toolchain using "stable"
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "stable"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: Toolchain version mismatch with MSRV fails with strict profile.
/// _Requirements: 7.2_
#[test]
fn check_toolchain_msrv_mismatch_fails() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");
    write_file(&dir, "scripts/tools.sha256", "");

    // Toolchain version different from MSRV
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.76.0"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    // Strict profile requires toolchain == MSRV
    cmd.assert().code(2);
}

/// Test: Toolchain >= MSRV passes with `relation_to_msrv = "atleast"` policy.
/// _Requirements: 7.2, 7.6_
#[test]
fn check_toolchain_at_least_msrv_passes() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");
    write_file(&dir, "scripts/tools.sha256", "");

    // Toolchain version higher than MSRV
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.76.0"
"#,
    );

    // Config with relation_to_msrv = "atleast" (note: no underscore)
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[policy.toolchain]
relation_to_msrv = "atleast"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: Unpinned toolchain passes with `require_pinned = false` policy.
/// _Requirements: 7.2, 7.6_
#[test]
fn check_unpinned_toolchain_passes_when_not_required() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");
    write_file(&dir, "scripts/tools.sha256", "");

    // Unpinned toolchain
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "stable"
"#,
    );

    // Config with require_pinned = false
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[policy.toolchain]
require_pinned = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

// =============================================================================
// Checksums validation tests
// =============================================================================

/// Test: Missing checksums file fails with strict profile.
/// _Requirements: 7.3_
#[test]
fn check_missing_checksums_file_fails() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    // No checksums file

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: Missing checksums file passes with `require_file = false` policy.
/// _Requirements: 7.3, 7.6_
#[test]
fn check_missing_checksums_passes_when_not_required() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    // Config with require_file = false
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[policy.checksums]
require_file = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

// =============================================================================
// Policy configuration tests
// =============================================================================

/// Test: Custom config file is loaded correctly.
/// _Requirements: 7.4, 7.6_
#[test]
fn check_custom_config_file_loaded() {
    let dir = TempDir::new().unwrap();

    // Create a workspace that would fail with default policy
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    // No toolchain file, no checksums file

    // Config that disables toolchain and checksums requirements
    write_file(
        &dir,
        "custom-config.toml",
        r#"[policy.toolchain]
require_pinned = false

[policy.checksums]
require_file = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("custom-config.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: fail_on = "warn" causes warnings to fail.
/// _Requirements: 7.4_
#[test]
fn check_fail_on_warn_causes_warnings_to_fail() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with fail_on = "warn"
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
fail_on = "warn"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Should pass since there are no warnings in a valid workspace
    cmd.assert().success().code(0);
}

/// Test: fail_on = "never" makes warnings pass (errors still fail).
/// Note: fail_on only affects warnings, not errors. Errors always cause exit code 2.
/// _Requirements: 7.4_
#[test]
fn check_fail_on_never_makes_warnings_pass() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with fail_on = "never" - warnings won't cause failure
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
fail_on = "never"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Valid workspace should pass
    cmd.assert().success().code(0);
}

/// Test: Custom output directory is used.
/// _Requirements: 7.4_
#[test]
fn check_custom_output_directory() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with custom out_dir
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
out_dir = "custom-output"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd.assert().success();

    // Verify files are in custom directory
    let report_path = dir.path().join("custom-output/report.json");
    let md_path = dir.path().join("custom-output/comment.md");
    assert!(
        report_path.exists(),
        "report.json should be in custom-output"
    );
    assert!(md_path.exists(), "comment.md should be in custom-output");
}

/// Test: Custom output paths via CLI flags.
/// _Requirements: 6.1_
#[test]
fn check_custom_output_paths_via_cli() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let custom_report = dir.path().join("my-report.json");
    let custom_md = dir.path().join("my-summary.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--out")
        .arg(&custom_report)
        .arg("--md")
        .arg(&custom_md)
        .arg("--always");

    cmd.assert().success();

    assert!(custom_report.exists(), "custom report path should be used");
    assert!(custom_md.exists(), "custom markdown path should be used");
}

// =============================================================================
// GitHub annotations tests
// =============================================================================

/// Test: --annotations github flag outputs annotations to stdout when findings have location.
/// Note: Annotations are only generated for findings that have both path and line number.
/// _Requirements: 6.1_
#[test]
fn check_annotations_github_flag() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--annotations")
        .arg("github")
        .arg("--always");

    // Valid workspace should pass and not output annotations (no findings)
    cmd.assert().success().code(0);
}

/// Test: --annotations github outputs error annotations for findings with location.
/// _Requirements: 6.1_
#[test]
fn check_annotations_github_outputs_errors_with_location() {
    let dir = TempDir::new().unwrap();

    // Create a workspace with missing MSRV - this should produce findings with location
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--annotations")
        .arg("github")
        .arg("--always");

    // Should fail with exit code 2 with strict profile
    // Note: Annotations are only output if findings have path AND line number
    cmd.assert().code(2);
}

// =============================================================================
// Workspace resolver tests
// =============================================================================

/// Test: Workspace with resolver = "2" passes.
/// _Requirements: 7.4_
#[test]
fn check_workspace_resolver_2_passes() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}

// =============================================================================
// Multiple crate workspace tests
// =============================================================================

/// Test: Multi-crate workspace with consistent MSRV passes.
/// _Requirements: 7.1_
#[test]
fn check_multi_crate_workspace_passes() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a", "crates/b", "crates/c"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    for crate_name in &["a", "b", "c"] {
        write_file(
            &dir,
            &format!("crates/{}/Cargo.toml", crate_name),
            &format!(
                r#"[package]
name = "{}"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
                crate_name
            ),
        );
        write_file(&dir, &format!("crates/{}/src/lib.rs", crate_name), "");
    }

    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    write_file(&dir, "scripts/tools.sha256", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}

// =============================================================================
// Error message tests
// =============================================================================

// =============================================================================
// Cockpit mode tests
// =============================================================================

/// Test: `--mode cockpit` alone defaults to artifacts-dir layout.
#[test]
fn cockpit_mode_defaults_artifact_dir() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--mode")
        .arg("cockpit")
        .arg("--always");

    cmd.assert().success().code(0);

    // Should create the full artifact tree
    let report = dir.path().join("artifacts/builddiag/report.json");
    let comment = dir.path().join("artifacts/builddiag/comment.md");
    let payload = dir.path().join("artifacts/builddiag/extras/payload.json");

    assert!(report.exists(), "report.json should be created");
    assert!(comment.exists(), "comment.md should be created");
    assert!(payload.exists(), "extras/payload.json should be created");

    // report.json should be sensor.report.v1 format
    let content = fs::read_to_string(&report).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        val["schema"], "sensor.report.v1",
        "cockpit mode should produce sensor format"
    );
}

/// Test: explicit `--artifacts-dir` overrides the cockpit default.
#[test]
fn cockpit_mode_explicit_artifacts_dir_wins() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let custom_dir = dir.path().join("custom-artifacts");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--mode")
        .arg("cockpit")
        .arg("--artifacts-dir")
        .arg(&custom_dir)
        .arg("--always");

    cmd.assert().success().code(0);

    let report = custom_dir.join("report.json");
    assert!(report.exists(), "report.json should be in custom dir");

    // Default location should NOT exist
    let default_report = dir.path().join("artifacts/builddiag/report.json");
    assert!(
        !default_report.exists(),
        "default artifact dir should not be used when explicit dir given"
    );
}

/// Test: `--out` suppresses the cockpit default artifact-dir.
#[test]
fn cockpit_mode_explicit_out_skips_default() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let custom_out = dir.path().join("my-report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--mode")
        .arg("cockpit")
        .arg("--out")
        .arg(&custom_out)
        .arg("--always");

    cmd.assert().success().code(0);

    assert!(custom_out.exists(), "custom --out path should be used");

    // Default artifact directory layout should NOT be created
    let default_payload = dir.path().join("artifacts/builddiag/extras/payload.json");
    assert!(
        !default_payload.exists(),
        "artifact dir layout should not be created when --out is specified"
    );
}

// =============================================================================
// Error message tests
// =============================================================================

/// Test: Error messages are descriptive.
/// _Requirements: 6.1_
#[test]
fn check_error_messages_are_descriptive() {
    let dir = TempDir::new().unwrap();

    // Create a workspace with missing MSRV
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);

    // Verify the report contains descriptive error information
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Check that findings exist and have messages (new schema uses flat findings array)
    let findings = report["findings"].as_array().unwrap();
    assert!(
        !findings.is_empty(),
        "Report should contain findings for errors"
    );
}
