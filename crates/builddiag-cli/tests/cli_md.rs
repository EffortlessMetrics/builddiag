//! Integration tests for the `builddiag md` command.
//!
//! These tests validate the CLI behavior for rendering Markdown from JSON reports.
//!
//! _Requirements: 6.2_

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

/// Creates a valid JSON report file for testing (passing report).
fn create_valid_report(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-001",
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z"
  },
  "repo": {
    "root": "/test/repo",
    "detected": {
      "is_workspace": true,
      "members": 1
    }
  },
  "inputs": {
    "cargo_root": "Cargo.toml",
    "rust_toolchain": "rust-toolchain.toml",
    "tools_checksums": "scripts/tools.sha256",
    "tools_manifest": null
  },
  "checks": [
    {
      "id": "msrv.workspace_msrv_defined",
      "status": "pass",
      "findings": [],
      "skipped_reason": null
    },
    {
      "id": "toolchain.pinned",
      "status": "pass",
      "findings": [],
      "skipped_reason": null
    }
  ],
  "summary": {
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 0
    },
    "verdict": "pass",
    "reasons": []
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with findings for testing (failing report).
fn create_report_with_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-002",
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z"
  },
  "repo": {
    "root": "/test/repo",
    "detected": {
      "is_workspace": true,
      "members": 1
    }
  },
  "inputs": {
    "cargo_root": "Cargo.toml",
    "rust_toolchain": "rust-toolchain.toml",
    "tools_checksums": null,
    "tools_manifest": null
  },
  "checks": [
    {
      "id": "msrv.workspace_msrv_defined",
      "status": "fail",
      "findings": [
        {
          "severity": "error",
          "code": "missing_msrv",
          "message": "No rust-version found in workspace Cargo.toml",
          "path": "Cargo.toml",
          "line": 1,
          "column": null
        }
      ],
      "skipped_reason": null
    },
    {
      "id": "toolchain.pinned",
      "status": "pass",
      "findings": [],
      "skipped_reason": null
    }
  ],
  "summary": {
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 1
    },
    "verdict": "fail",
    "reasons": ["1 error found"]
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

// =============================================================================
// Basic md command tests
// =============================================================================

/// Test: `builddiag md` with valid JSON report outputs to stdout.
/// _Requirements: 6.2_
#[test]
fn md_valid_report_outputs_to_stdout() {
    let dir = TempDir::new().unwrap();
    let report_path = create_valid_report(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("builddiag"))
        .stdout(predicate::str::contains("pass").or(predicate::str::contains("Pass")));
}

/// Test: `builddiag md` with valid JSON report outputs to file.
/// _Requirements: 6.2_
#[test]
fn md_valid_report_outputs_to_file() {
    let dir = TempDir::new().unwrap();
    let report_path = create_valid_report(&dir, "report.json");
    let output_path = dir.path().join("output.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md")
        .arg("--report")
        .arg(&report_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert().success();

    // Verify the output file was created
    assert!(output_path.exists(), "output.md should be created");

    // Verify the content is valid markdown
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(
        content.contains("builddiag"),
        "Markdown should contain builddiag header"
    );
}

/// Test: `builddiag md` with report containing findings includes them in output.
/// _Requirements: 6.2_
#[test]
fn md_report_with_findings_includes_findings() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fail").or(predicate::str::contains("Fail")))
        .stdout(predicate::str::contains("error").or(predicate::str::contains("Error")));
}

// =============================================================================
// Error handling tests
// =============================================================================

/// Test: `builddiag md` with missing report file fails with error.
/// _Requirements: 6.2_
#[test]
fn md_missing_report_file_fails() {
    let dir = TempDir::new().unwrap();
    let nonexistent_path = dir.path().join("nonexistent.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&nonexistent_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test: `builddiag md` with invalid JSON fails with error.
/// _Requirements: 6.2_
#[test]
fn md_invalid_json_fails() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "invalid.json", "{ not valid json }");
    let report_path = dir.path().join("invalid.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

/// Test: `builddiag md` with valid JSON but wrong schema fails.
/// _Requirements: 6.2_
#[test]
fn md_wrong_json_schema_fails() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "wrong_schema.json", r#"{"foo": "bar"}"#);
    let report_path = dir.path().join("wrong_schema.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

/// Test: `builddiag md` without --report argument fails.
/// _Requirements: 6.2_
#[test]
fn md_missing_report_argument_fails() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("md");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--report"));
}

// =============================================================================
// Integration with check command tests
// =============================================================================

/// Test: `builddiag md` can render report generated by `builddiag check`.
/// _Requirements: 6.2_
#[test]
fn md_renders_check_generated_report() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // First, run check to generate a report
    let report_path = dir.path().join("report.json");
    let mut check_cmd = get_builddiag_cmd();
    check_cmd
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--out")
        .arg(&report_path)
        .arg("--always");

    check_cmd.assert().success();
    assert!(report_path.exists(), "check should create report.json");

    // Now, use md command to render the report
    let mut md_cmd = get_builddiag_cmd();
    md_cmd.arg("md").arg("--report").arg(&report_path);

    md_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("builddiag"));
}

/// Test: `builddiag md` output matches `builddiag check --md` output.
/// _Requirements: 6.2_
#[test]
fn md_output_matches_check_md_output() {
    let dir = TempDir::new().unwrap();
    create_valid_workspace(&dir);

    // Run check to generate both report.json and comment.md
    let report_path = dir.path().join("report.json");
    let check_md_path = dir.path().join("check_comment.md");
    let mut check_cmd = get_builddiag_cmd();
    check_cmd
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--out")
        .arg(&report_path)
        .arg("--md")
        .arg(&check_md_path)
        .arg("--always");

    check_cmd.assert().success();

    // Run md command to generate markdown from the report
    let md_output_path = dir.path().join("md_comment.md");
    let mut md_cmd = get_builddiag_cmd();
    md_cmd
        .arg("md")
        .arg("--report")
        .arg(&report_path)
        .arg("--out")
        .arg(&md_output_path);

    md_cmd.assert().success();

    // Compare the outputs - they should be identical
    let check_md_content = fs::read_to_string(&check_md_path).unwrap();
    let md_output_content = fs::read_to_string(&md_output_path).unwrap();

    assert_eq!(
        check_md_content, md_output_content,
        "md command output should match check --md output"
    );
}

// =============================================================================
// Output format tests
// =============================================================================

/// Test: `builddiag md` output is valid Markdown.
/// _Requirements: 6.2_
#[test]
fn md_output_is_valid_markdown() {
    let dir = TempDir::new().unwrap();
    let report_path = create_valid_report(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let content = String::from_utf8(output).unwrap();

    // Basic Markdown validation - should contain headers
    assert!(
        content.contains('#'),
        "Markdown should contain header markers"
    );
}

/// Test: `builddiag md` output contains expected sections.
/// _Requirements: 6.2_
#[test]
fn md_output_contains_expected_sections() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    cmd.assert()
        .success()
        // Should contain check information
        .stdout(predicate::str::contains("msrv").or(predicate::str::contains("MSRV")));
}

/// Test: `builddiag md` stdout output has no trailing newline issues.
/// _Requirements: 6.2_
#[test]
fn md_stdout_output_format_is_clean() {
    let dir = TempDir::new().unwrap();
    let report_path = create_valid_report(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let content = String::from_utf8(output).unwrap();

    // Output should not be empty
    assert!(!content.is_empty(), "Output should not be empty");

    // Output should be valid UTF-8 (already verified by from_utf8)
    // Output should not have excessive whitespace at the end
    let trimmed = content.trim_end();
    assert!(
        content.len() - trimmed.len() <= 2,
        "Output should not have excessive trailing whitespace"
    );
}

// =============================================================================
// Edge case tests
// =============================================================================

/// Test: `builddiag md` with empty checks array.
/// _Requirements: 6.2_
#[test]
fn md_empty_checks_array() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-003",
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z"
  },
  "repo": {
    "root": "/test/repo",
    "detected": {
      "is_workspace": true,
      "members": 1
    }
  },
  "inputs": {
    "cargo_root": "Cargo.toml",
    "rust_toolchain": null,
    "tools_checksums": null,
    "tools_manifest": null
  },
  "checks": [],
  "summary": {
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 0
    },
    "verdict": "pass",
    "reasons": []
  }
}"#;
    write_file(&dir, "empty_checks.json", report);
    let report_path = dir.path().join("empty_checks.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md").arg("--report").arg(&report_path);

    // Should succeed even with empty checks
    cmd.assert().success();
}

/// Test: `builddiag md` output to nested directory creates parent directories.
/// _Requirements: 6.2_
#[test]
fn md_output_to_nested_directory() {
    let dir = TempDir::new().unwrap();
    let report_path = create_valid_report(&dir, "report.json");
    let output_path = dir.path().join("nested/deep/output.md");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("md")
        .arg("--report")
        .arg(&report_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert().success();

    // Verify the output file was created in the nested directory
    assert!(
        output_path.exists(),
        "output.md should be created in nested directory"
    );
}

/// Test: `builddiag md` is deterministic - same input produces same output.
/// _Requirements: 6.2_
#[test]
fn md_output_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_findings(&dir, "report.json");

    // Run md command twice
    let mut cmd1 = get_builddiag_cmd();
    cmd1.arg("md").arg("--report").arg(&report_path);
    let output1 = cmd1.assert().success().get_output().stdout.clone();

    let mut cmd2 = get_builddiag_cmd();
    cmd2.arg("md").arg("--report").arg(&report_path);
    let output2 = cmd2.assert().success().get_output().stdout.clone();

    assert_eq!(output1, output2, "md output should be deterministic");
}
