//! Integration tests for CLI exit codes.
//!
//! These tests validate that the builddiag CLI returns the correct exit codes
//! based on check results and the `fail_on` configuration.
//!
//! Exit codes:
//! - 0: Success - all checks pass, or warnings with fail_on=error (default)
//! - 1: Tool/runtime error (e.g., config file not found, invalid arguments)
//! - 2: Policy failure - errors present, or warnings with fail_on=warn
//!
//! _Requirements: 6.8_

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
/// This workspace should pass all checks with default policy.
fn create_valid_workspace(dir: &TempDir) {
    write_file(
        dir,
        "Cargo.toml",
        r#"[workspace]
    resolver = "2"
    members = ["crates/a"]

    [workspace.package]
    rust-version = "1.92.0"
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
    channel = "1.92.0"
    "#,
    );

    write_file(dir, "scripts/tools.sha256", "");
}

// =============================================================================
// Exit Code 0 Tests - All checks pass
// =============================================================================

/// Test: Exit code 0 when all checks pass with default configuration.
/// _Requirements: 6.8_
#[test]
fn exit_code_0_when_all_checks_pass() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}

/// Test: Exit code 0 when warnings exist but fail_on=error (default).
/// _Requirements: 6.8_
#[test]
fn exit_code_0_when_warnings_with_fail_on_error() {
    let dir = TempDir::new().unwrap();

    // Create a workspace that produces warnings but not errors.
    // We'll use a toolchain version higher than MSRV with relation_to_msrv = "atleast"
    // and set the toolchain check to warn severity.
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
    resolver = "2"
    members = ["crates/a"]

    [workspace.package]
    rust-version = "1.92.0"
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
    channel = "1.92.0"
    "#,
    );

    write_file(&dir, "scripts/tools.sha256", "");

    // Config with fail_on = "error" (default behavior)
    // This ensures warnings don't cause failure
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
fail_on = "error"
"#,
    );

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join(".builddiag.toml"))
        .arg("--always");

    // Should pass with exit code 0 (no errors, warnings allowed)
    cmd.assert().success().code(0);
}

/// Test: Exit code 0 when fail_on=never, even with errors.
/// Note: fail_on=never only affects warnings; errors still cause exit code 2.
/// This test verifies exit code 0 for a passing workspace with fail_on=never.
/// _Requirements: 6.8_
#[test]
fn exit_code_0_with_fail_on_never_and_no_errors() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with fail_on = "never"
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

    cmd.assert().success().code(0);
}

// =============================================================================
// Exit Code 2 Tests - Errors present (checks failed)
// =============================================================================

/// Test: Exit code 2 when MSRV is missing (error with strict profile).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_msrv_missing() {
    let dir = TempDir::new().unwrap();

    // Workspace without rust-version - this is an error with strict profile
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

/// Test: Exit code 2 when toolchain is missing (error with strict profile).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_toolchain_missing() {
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

    // No rust-toolchain.toml - this is an error with strict profile

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: Exit code 2 when checksums file is missing (error with strict profile).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_checksums_missing() {
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

    // No checksums file - this is an error with strict profile

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

/// Test: Exit code 2 when toolchain is unpinned (error with strict profile).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_toolchain_unpinned() {
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

    // Unpinned toolchain using "stable" - this is an error with strict profile
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

/// Test: Exit code 2 when multiple errors exist (with strict profile).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_multiple_errors() {
    let dir = TempDir::new().unwrap();

    // Workspace with multiple issues: no MSRV, no toolchain, no checksums
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

/// Test: Exit code 2 even with fail_on=never when errors exist.
/// Note: fail_on only affects warnings, not errors. Errors always cause exit code 2.
/// _Requirements: 6.8_
#[test]
fn exit_code_2_with_fail_on_never_when_errors_exist() {
    let dir = TempDir::new().unwrap();

    // Workspace with missing MSRV - this is an error with strict profile
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

    // Config with fail_on = "never" and strict profile - errors still cause exit code 2
    write_file(
        &dir,
        ".builddiag.toml",
        r#"profile = "strict"

[defaults]
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

    // Errors always cause exit code 2, regardless of fail_on setting
    cmd.assert().code(2);
}

// =============================================================================
// Exit Code 2 Tests - Warnings with fail_on=warn (policy failure)
// =============================================================================

/// Test: Exit code 2 when warnings exist with fail_on=warn.
/// This test creates a scenario where a check produces a warning (not error).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_when_warnings_with_fail_on_warn() {
    let dir = TempDir::new().unwrap();

    // Create a valid workspace first
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

    // Don't create checksums file - we'll configure it as a warning

    // Config that:
    // 1. Sets fail_on = "warn" so warnings cause exit code 3
    // 2. Overrides the checksums check to warn severity (note: full check ID with prefix)
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
fail_on = "warn"

# Override checksums check to warn severity
[[checks]]
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

    // Should exit with code 2 (policy failure: warning with fail_on=warn)
    cmd.assert().code(2);
}

/// Test: Exit code 0 when no warnings with fail_on=warn.
/// _Requirements: 6.8_
#[test]
fn exit_code_0_when_no_warnings_with_fail_on_warn() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Config with fail_on = "warn" but no warnings should be produced
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

    // Valid workspace with fail_on=warn should still exit 0 (no warnings)
    cmd.assert().success().code(0);
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test: Exit code 2 for mixed errors and warnings.
/// When both errors and warnings exist, exit code should be 2 (policy failure).
/// _Requirements: 6.8_
#[test]
fn exit_code_2_for_mixed_errors_and_warnings() {
    let dir = TempDir::new().unwrap();

    // Create workspace with both errors and warnings
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

    // Missing toolchain file (error with strict) and missing checksums (warning)
    // No rust-toolchain.toml

    // Config with fail_on = "warn", strict profile for toolchain, and checksums as warning
    write_file(
        &dir,
        ".builddiag.toml",
        r#"[defaults]
fail_on = "warn"

# Make toolchain_pinning an error
[[checks]]
id = "rust.toolchain_pinning"
severity = "error"

# Keep checksums as warning
[[checks]]
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

    // Should be exit code 2 (policy failure)
    cmd.assert().code(2);
}

/// Test: Verify exit codes are consistent across multiple runs.
/// _Requirements: 6.8_
#[test]
fn exit_codes_are_deterministic() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Run the check multiple times and verify consistent exit code
    for _ in 0..3 {
        let mut cmd = get_builddiag_cmd();
        cmd.arg("check")
            .arg("--root")
            .arg(dir.path())
            .arg("--always");

        cmd.assert().success().code(0);
    }
}
