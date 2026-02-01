//! Integration tests for diff-aware mode and the --always flag.
//!
//! These tests validate the CLI behavior for diff-aware mode which uses git to detect
//! changed files and only runs relevant checks, as well as the --always flag which
//! forces all checks to run regardless of changed files.
//!
//! Note: Testing actual git diff behavior is complex, so these tests focus on:
//! - The --always flag behavior
//! - Config-based diff_aware setting
//! - Graceful handling when git is not available (not a git repository)
//!
//! _Requirements: 6.4, 7.7_

use assert_cmd::Command;
use predicates::prelude::*;
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
// --always flag tests
// =============================================================================

/// Test: --always flag forces all checks to run.
/// When --always is provided, all checks should run regardless of diff-aware mode.
/// _Requirements: 6.4, 7.7_
#[test]
fn always_flag_forces_all_checks_to_run() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);

    // Verify report was created with all checks
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(report_path.exists(), "report.json should be created");

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify checks were run
    let checks = report["checks"].as_array().unwrap();
    assert!(!checks.is_empty(), "checks should be present in report");
}

/// Test: --always flag works with --diff-aware flag.
/// When both flags are provided, --always should take precedence.
/// _Requirements: 6.4, 7.7_
#[test]
fn always_flag_overrides_diff_aware_flag() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--always");

    // Should succeed because --always forces all checks to run
    cmd.assert().success().code(0);

    // Verify report was created
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(report_path.exists(), "report.json should be created");
}

/// Test: --always flag works with diff_aware config option.
/// When config has diff_aware = true but --always is provided, all checks should run.
/// _Requirements: 6.4, 7.7_
#[test]
fn always_flag_overrides_diff_aware_config() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with diff_aware = true
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = true
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Should succeed because --always forces all checks to run
    cmd.assert().success().code(0);

    // Verify report was created with checks
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    let checks = report["checks"].as_array().unwrap();
    assert!(!checks.is_empty(), "checks should be present in report");
}

/// Test: Without --always flag, checks still run (default behavior).
/// This verifies the baseline behavior without the --always flag.
/// _Requirements: 6.4_
#[test]
fn without_always_flag_checks_still_run() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Note: Without --always and without being in a git repo, diff-aware mode
    // will fail open and run all checks anyway
    let mut cmd = get_builddiag_cmd();
    cmd.arg("check").arg("--root").arg(dir.path());

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

// =============================================================================
// diff_aware config option tests
// =============================================================================

/// Test: diff_aware config option is recognized.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_config_option_is_recognized() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with diff_aware = true
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = true
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"));

    // Should succeed (not a git repo, so diff-aware fails open and runs all checks)
    cmd.assert().success().code(0);
}

/// Test: diff_aware = false in config disables diff-aware mode.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_false_config_disables_diff_aware() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with diff_aware = false (explicit)
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"));

    // Should succeed with all checks running
    cmd.assert().success().code(0);

    // Verify report was created
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(report_path.exists(), "report.json should be created");
}

/// Test: --diff-aware CLI flag enables diff-aware mode.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_cli_flag_enables_diff_aware() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware");

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

/// Test: --diff-aware CLI flag overrides config diff_aware = false.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_cli_flag_overrides_config() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with diff_aware = false
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--diff-aware");

    // Should succeed (CLI flag enables diff-aware, but not a git repo so fails open)
    cmd.assert().success().code(0);
}

// =============================================================================
// Non-git repository behavior tests
// =============================================================================

/// Test: Diff-aware mode gracefully handles non-git repositories.
/// When not in a git repository, diff-aware mode should fail open and run all checks.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_gracefully_handles_non_git_repo() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Enable diff-aware mode via CLI flag
    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware");

    // Should succeed because diff-aware fails open when git is not available
    cmd.assert().success().code(0);

    // Verify report was created with checks (all checks should run)
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(
        report_path.exists(),
        "report.json should be created even without git"
    );

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    let checks = report["checks"].as_array().unwrap();
    assert!(
        !checks.is_empty(),
        "checks should run when diff-aware fails open"
    );
}

/// Test: Diff-aware mode with config in non-git repository.
/// _Requirements: 6.4, 7.7_
#[test]
fn diff_aware_config_in_non_git_repo() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with diff_aware = true
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = true
base = "origin/main"
head = "HEAD"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"));

    // Should succeed because diff-aware fails open when git is not available
    cmd.assert().success().code(0);
}

// =============================================================================
// Base and head ref configuration tests
// =============================================================================

/// Test: Custom base ref can be specified via CLI.
/// _Requirements: 6.4, 7.7_
#[test]
fn custom_base_ref_via_cli() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--base")
        .arg("origin/develop");

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

/// Test: Custom head ref can be specified via CLI.
/// _Requirements: 6.4, 7.7_
#[test]
fn custom_head_ref_via_cli() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--head")
        .arg("feature-branch");

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

/// Test: Custom base and head refs can be specified via config.
/// _Requirements: 6.4, 7.7_
#[test]
fn custom_base_and_head_refs_via_config() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with custom base and head refs
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = true
base = "origin/develop"
head = "feature-branch"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"));

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

/// Test: CLI refs override config refs.
/// _Requirements: 6.4, 7.7_
#[test]
fn cli_refs_override_config_refs() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with base and head refs
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
diff_aware = true
base = "origin/main"
head = "HEAD"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--base")
        .arg("origin/develop")
        .arg("--head")
        .arg("feature-branch");

    // Should succeed (not a git repo, so diff-aware fails open)
    cmd.assert().success().code(0);
}

// =============================================================================
// Error handling tests
// =============================================================================

/// Test: Invalid base ref doesn't crash the tool.
/// When git diff fails, the tool should fail open and run all checks.
/// _Requirements: 6.4, 7.7_
#[test]
fn invalid_base_ref_fails_open() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--base")
        .arg("nonexistent/branch");

    // Should succeed because diff-aware fails open
    cmd.assert().success().code(0);
}

/// Test: Invalid head ref doesn't crash the tool.
/// _Requirements: 6.4, 7.7_
#[test]
fn invalid_head_ref_fails_open() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--head")
        .arg("nonexistent-branch");

    // Should succeed because diff-aware fails open
    cmd.assert().success().code(0);
}

// =============================================================================
// Combined flag tests
// =============================================================================

/// Test: All diff-aware related flags can be combined.
/// _Requirements: 6.4, 7.7_
#[test]
fn all_diff_aware_flags_combined() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--base")
        .arg("origin/main")
        .arg("--head")
        .arg("HEAD")
        .arg("--always");

    // Should succeed with --always taking precedence
    cmd.assert().success().code(0);
}

/// Test: Diff-aware mode with custom output paths.
/// _Requirements: 6.4_
#[test]
fn diff_aware_with_custom_output_paths() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let custom_report = dir.path().join("custom-report.json");
    let custom_md = dir.path().join("custom-summary.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--out")
        .arg(&custom_report)
        .arg("--md")
        .arg(&custom_md);

    cmd.assert().success();

    assert!(
        custom_report.exists(),
        "custom report path should be created"
    );
    assert!(custom_md.exists(), "custom markdown path should be created");
}

/// Test: Diff-aware mode with GitHub annotations.
/// _Requirements: 6.4_
#[test]
fn diff_aware_with_github_annotations() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--diff-aware")
        .arg("--github-annotations");

    // Should succeed (valid workspace, no findings to annotate)
    cmd.assert().success().code(0);
}

// =============================================================================
// Determinism tests
// =============================================================================

/// Test: Diff-aware mode produces deterministic output.
/// Running the same check multiple times should produce identical results.
/// _Requirements: 6.4_
#[test]
fn diff_aware_produces_deterministic_output() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Run check twice with diff-aware mode
    let mut reports = Vec::new();
    for _ in 0..2 {
        let mut cmd = get_builddiag_cmd();
        cmd.arg("check")
            .arg("--root")
            .arg(dir.path())
            .arg("--diff-aware")
            .arg("--always");

        cmd.assert().success();

        let report_path = dir.path().join("artifacts/builddiag/report.json");
        let content = fs::read_to_string(&report_path).unwrap();
        let report: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Extract checks and summary for comparison (excluding timestamps)
        let checks = report["checks"].clone();
        let summary = report["summary"].clone();
        reports.push((checks, summary));
    }

    // Verify both runs produced the same checks and summary
    assert_eq!(
        reports[0].0, reports[1].0,
        "checks should be identical across runs"
    );
    assert_eq!(
        reports[0].1, reports[1].1,
        "summary should be identical across runs"
    );
}

// =============================================================================
// Help text tests
// =============================================================================

/// Test: --help shows diff-aware related options.
/// _Requirements: 6.4_
#[test]
fn help_shows_diff_aware_options() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("check").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--diff-aware"))
        .stdout(predicate::str::contains("--base"))
        .stdout(predicate::str::contains("--head"))
        .stdout(predicate::str::contains("--always"));
}
