//! Integration tests for CLI configuration file handling.
//!
//! These tests validate the `--config` flag behavior, config file loading,
//! and override behavior between config files and CLI flags.
//!
//! _Requirements: 6.5, 7.6_

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

/// Creates a workspace that would fail with default policy (missing toolchain and checksums).
fn create_workspace_missing_toolchain_and_checksums(dir: &TempDir) {
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
    write_file(dir, "crates/a/src/lib.rs", "");

    // No rust-toolchain.toml
    // No checksums file
}

// =============================================================================
// --config flag with valid config file
// =============================================================================

/// Test: --config flag with valid config file is loaded correctly.
/// _Requirements: 6.5, 7.6_
#[test]
fn config_flag_with_valid_config_file() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_toolchain_and_checksums(&dir);

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

    // Should pass because config disables the failing checks
    cmd.assert().success().code(0);
}

/// Test: --config flag with config file in different directory.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_config_in_different_directory() {
    let dir = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();

    create_workspace_missing_toolchain_and_checksums(&dir);

    // Config in a completely different directory
    let config_path = config_dir.path().join("external-config.toml");
    fs::write(
        &config_path,
        r#"[policy.toolchain]
require_pinned = false

[policy.checksums]
require_file = false
"#,
    )
    .unwrap();

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: --config flag with config file containing all sections.
/// _Requirements: 6.5, 7.6_
#[test]
fn config_flag_with_comprehensive_config_file() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Comprehensive config with all sections
    write_file(
        &dir,
        "full-config.toml",
        r#"[defaults]
fail_on = "error"
out_dir = "custom-output"
diff_aware = false
base = "origin/main"
head = "HEAD"

[paths]
cargo_root = "Cargo.toml"
rust_toolchain = "rust-toolchain.toml"
tools_checksums = "scripts/tools.sha256"

[policy.msrv]
require_defined = true
source = "workspace"

[policy.toolchain]
require_pinned = true
relation_to_msrv = "equals"
allow_nightly = false

[policy.checksums]
require_file = true

[meta]
project = "test-project"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("full-config.toml"))
        .arg("--always");

    cmd.assert().success();

    // Verify custom output directory was used
    let report_path = dir.path().join("custom-output/report.json");
    assert!(
        report_path.exists(),
        "report.json should be in custom-output directory"
    );
}

/// Test: --config flag with empty config file uses defaults.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_empty_config_file() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Empty config file should use all defaults
    write_file(&dir, "empty-config.toml", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("empty-config.toml"))
        .arg("--always");

    cmd.assert().success().code(0);

    // Verify default output directory was used
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(
        report_path.exists(),
        "report.json should be in default artifacts/builddiag directory"
    );
}

// =============================================================================
// --config flag with missing config file (error)
// =============================================================================

/// Test: --config flag with non-existent config file returns error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_missing_config_file_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("nonexistent-config.toml"))
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("read config"));
}

/// Test: --config flag with missing config file includes path in error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_missing_config_file_shows_path_in_error() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let config_path = dir.path().join("missing.toml");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(&config_path)
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("missing.toml"));
}

// =============================================================================
// --config flag with invalid config file (error)
// =============================================================================

/// Test: --config flag with invalid TOML syntax returns error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_invalid_toml_syntax_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Invalid TOML syntax (missing closing bracket)
    write_file(
        &dir,
        "invalid-syntax.toml",
        r#"[defaults
fail_on = "error"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("invalid-syntax.toml"))
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse config"));
}

/// Test: --config flag with invalid enum value returns error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_invalid_enum_value_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Valid TOML syntax but invalid enum value
    write_file(
        &dir,
        "invalid-enum.toml",
        r#"[defaults]
fail_on = "invalid_value"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("invalid-enum.toml"))
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse config"));
}

/// Test: --config flag with wrong type value returns error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_wrong_type_value_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Valid TOML syntax but wrong type (string instead of bool)
    write_file(
        &dir,
        "wrong-type.toml",
        r#"[defaults]
diff_aware = "yes"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("wrong-type.toml"))
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse config"));
}

/// Test: --config flag with deeply nested invalid structure returns error.
/// _Requirements: 6.5_
#[test]
fn config_flag_with_invalid_nested_structure_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Invalid nested structure - checks should be an array of tables, not a table
    write_file(
        &dir,
        "invalid-structure.toml",
        r#"[checks]
id = "some.check"
severity = "warn"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("invalid-structure.toml"))
        .arg("--always");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse config"));
}

// =============================================================================
// Config file override behavior (CLI flags override config)
// =============================================================================

/// Test: CLI --out flag overrides config out_dir.
/// _Requirements: 7.6_
#[test]
fn cli_out_flag_overrides_config_out_dir() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with custom out_dir
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
out_dir = "config-output"
"#,
    );

    let custom_report = dir.path().join("cli-output/report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--out")
        .arg(&custom_report)
        .arg("--always");

    cmd.assert().success();

    // CLI --out should override config out_dir
    assert!(
        custom_report.exists(),
        "CLI --out should override config out_dir"
    );

    // Config out_dir should NOT be used
    let config_report = dir.path().join("config-output/report.json");
    assert!(
        !config_report.exists(),
        "config out_dir should not be used when --out is specified"
    );
}

/// Test: CLI --md flag overrides config markdown output.
/// _Requirements: 7.6_
#[test]
fn cli_md_flag_overrides_config_output() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with custom out_dir
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
out_dir = "config-output"
"#,
    );

    let custom_md = dir.path().join("cli-summary.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--md")
        .arg(&custom_md)
        .arg("--always");

    cmd.assert().success();

    // CLI --md should be used
    assert!(custom_md.exists(), "CLI --md path should be used");
}

/// Test: CLI --diff-aware flag overrides config diff_aware setting.
/// _Requirements: 7.6_
#[test]
fn cli_diff_aware_flag_overrides_config() {
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
        .arg("--diff-aware")
        .arg("--always");

    // Should succeed (--always ensures checks run even with diff-aware)
    cmd.assert().success().code(0);
}

/// Test: CLI --base and --head flags override config values.
/// _Requirements: 7.6_
#[test]
fn cli_base_head_flags_override_config() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with specific base and head
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
base = "origin/develop"
head = "feature-branch"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--base")
        .arg("origin/main")
        .arg("--head")
        .arg("HEAD")
        .arg("--always");

    // Should succeed (CLI flags override config)
    cmd.assert().success().code(0);
}

/// Test: Multiple CLI flags override multiple config values.
/// _Requirements: 7.6_
#[test]
fn multiple_cli_flags_override_config() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with various settings
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
out_dir = "config-output"
diff_aware = false
base = "origin/develop"
head = "feature-branch"
"#,
    );

    let custom_report = dir.path().join("cli-output/report.json");
    let custom_md = dir.path().join("cli-summary.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--out")
        .arg(&custom_report)
        .arg("--md")
        .arg(&custom_md)
        .arg("--base")
        .arg("origin/main")
        .arg("--head")
        .arg("HEAD")
        .arg("--always");

    cmd.assert().success();

    // Verify CLI flags were used
    assert!(custom_report.exists(), "CLI --out should be used");
    assert!(custom_md.exists(), "CLI --md should be used");
}

// =============================================================================
// Default config behavior when no --config is provided
// =============================================================================

/// Test: Default config is used when no --config flag is provided.
/// _Requirements: 6.5_
#[test]
fn default_config_when_no_config_flag() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);

    // Verify default output directory was used
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(
        report_path.exists(),
        "default artifacts/builddiag directory should be used"
    );
}

/// Test: Strict profile applies strict policy (requires MSRV, toolchain, checksums).
/// _Requirements: 6.5_
#[test]
fn default_config_applies_default_policy() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_toolchain_and_checksums(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    // Should fail because strict profile requires toolchain and checksums
    cmd.assert().code(2);
}

/// Test: .builddiag.toml in repo root is NOT auto-loaded (explicit --config required).
/// _Requirements: 6.5_
#[test]
fn builddiag_toml_in_root_not_auto_loaded() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_toolchain_and_checksums(&dir);

    // Create .builddiag.toml in repo root that would make checks pass
    write_file(
        &dir,
        ".builddiag.toml",
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
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    // Should fail because .builddiag.toml is NOT auto-loaded
    // (explicit --config flag is required)
    cmd.assert().code(2);
}

/// Test: Explicit --config flag is required to load config file.
/// _Requirements: 6.5_
#[test]
fn explicit_config_flag_required_to_load_config() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_toolchain_and_checksums(&dir);

    // Create config that would make checks pass
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[policy.toolchain]
require_pinned = false

[policy.checksums]
require_file = false
"#,
    );

    // Without --config flag and with strict profile, should fail
    let mut cmd_without_config = get_builddiag_cmd();
    cmd_without_config
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd_without_config.assert().code(2);

    // With --config flag, should pass (config disables those requirements)
    let mut cmd_with_config = get_builddiag_cmd();
    cmd_with_config
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    cmd_with_config.assert().success().code(0);
}

// =============================================================================
// Config file with check overrides
// =============================================================================

/// Test: Config file can override check severity.
/// _Requirements: 7.6_
#[test]
fn config_can_override_check_severity() {
    let dir = TempDir::new().unwrap();

    // Create workspace with missing checksums (would be error by default)
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

    // No checksums file - would be error by default

    // Config that overrides checksums check to warn severity
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[[checks]]
id = "tools.checksums_file_exists"
severity = "warn"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Should pass with exit code 0 (warning, not error) with default fail_on=error
    cmd.assert().success().code(0);
}

/// Test: Config file can disable specific checks.
/// _Requirements: 7.6_
#[test]
fn config_can_disable_specific_checks() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_toolchain_and_checksums(&dir);

    // Config that disables toolchain and checksums checks using correct check IDs
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[[checks]]
id = "rust.toolchain_pinning"
enabled = false

[[checks]]
id = "tools.checksums_file_exists"
enabled = false
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Should pass because the failing checks are disabled
    cmd.assert().success().code(0);
}

// =============================================================================
// Edge cases
// =============================================================================

/// Test: Config file with only comments is valid.
/// _Requirements: 6.5_
#[test]
fn config_file_with_only_comments_is_valid() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config file with only comments
    write_file(
        &dir,
        "comments-only.toml",
        r#"# This is a comment
# Another comment
# No actual configuration
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("comments-only.toml"))
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: Config file with partial sections uses defaults for missing fields.
/// _Requirements: 6.5_
#[test]
fn config_file_with_partial_sections_uses_defaults() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with only some fields set
    write_file(
        &dir,
        "partial-config.toml",
        r#"[defaults]
fail_on = "warn"
# out_dir not specified, should use default
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("partial-config.toml"))
        .arg("--always");

    cmd.assert().success();

    // Verify default out_dir was used
    let report_path = dir.path().join("artifacts/builddiag/report.json");
    assert!(
        report_path.exists(),
        "default out_dir should be used for missing field"
    );
}

/// Test: Config file path with spaces works correctly.
/// _Requirements: 6.5_
#[test]
fn config_file_path_with_spaces_works() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config file with spaces in path
    write_file(
        &dir,
        "config with spaces.toml",
        r#"[defaults]
out_dir = "output with spaces"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("config with spaces.toml"))
        .arg("--always");

    cmd.assert().success();

    // Verify output directory with spaces was created
    let report_path = dir.path().join("output with spaces/report.json");
    assert!(
        report_path.exists(),
        "output directory with spaces should be created"
    );
}
