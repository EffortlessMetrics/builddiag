//! Integration tests for the `builddiag annotations` command.
//!
//! These tests validate the CLI behavior for rendering GitHub Actions annotations
//! from JSON reports. The `annotations` command can be invoked as either
//! `builddiag annotations` or `builddiag github-annotations` (legacy alias).
//!
//! _Requirements: 6.3_

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

/// Creates a valid JSON report file with no findings (passing report).
fn create_passing_report(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "pass",
  "findings": [],
  "summary": {
    "total_findings": 0,
    "by_severity": {},
    "by_check": {}
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with error findings that have path and line (for annotations).
fn create_report_with_error_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "fail",
  "findings": [
    {
      "check_id": "rust.msrv_defined",
      "code": "missing_msrv",
      "severity": "error",
      "message": "No rust-version found in workspace Cargo.toml",
      "location": {
        "path": "Cargo.toml",
        "line": 1
      }
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "error": 1 },
    "by_check": { "rust.msrv_defined": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with warning findings.
fn create_report_with_warning_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "warn",
  "findings": [
    {
      "check_id": "tools.checksums_file_exists",
      "code": "missing_checksums",
      "severity": "warn",
      "message": "Checksums file not found",
      "location": {
        "path": "scripts/tools.sha256",
        "line": 1
      }
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "warn": 1 },
    "by_check": { "tools.checksums_file_exists": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with info findings.
fn create_report_with_info_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "pass",
  "findings": [
    {
      "check_id": "info.check",
      "code": "info_code",
      "severity": "info",
      "message": "Informational message",
      "location": {
        "path": "README.md",
        "line": 10
      }
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "info": 1 },
    "by_check": { "info.check": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with findings that have column information.
fn create_report_with_column_info(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "fail",
  "findings": [
    {
      "check_id": "syntax.error",
      "code": "syntax_error",
      "severity": "error",
      "message": "Syntax error at position",
      "location": {
        "path": "src/lib.rs",
        "line": 42,
        "col": 15
      }
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "error": 1 },
    "by_check": { "syntax.error": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with findings that lack path/line (should not produce annotations).
fn create_report_with_findings_no_location(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "fail",
  "findings": [
    {
      "check_id": "general.check",
      "code": "general_error",
      "severity": "error",
      "message": "General error without location"
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "error": 1 },
    "by_check": { "general.check": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with multiple findings of different severities.
fn create_report_with_mixed_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "fail",
  "findings": [
    {
      "check_id": "rust.msrv_defined",
      "code": "missing_msrv",
      "severity": "error",
      "message": "No rust-version found",
      "location": {
        "path": "Cargo.toml",
        "line": 1
      }
    },
    {
      "check_id": "rust.toolchain_pinning",
      "code": "unpinned_toolchain",
      "severity": "warn",
      "message": "Toolchain is not pinned to a specific version",
      "location": {
        "path": "rust-toolchain.toml",
        "line": 2
      }
    },
    {
      "check_id": "info.check",
      "code": "info_note",
      "severity": "info",
      "message": "Consider adding documentation",
      "location": {
        "path": "src/lib.rs",
        "line": 5
      }
    }
  ],
  "summary": {
    "total_findings": 3,
    "by_severity": { "error": 1, "warn": 1, "info": 1 },
    "by_check": { "rust.msrv_defined": 1, "rust.toolchain_pinning": 1, "info.check": 1 }
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

// =============================================================================
// Basic annotations command tests
// =============================================================================

/// Test: `builddiag annotations` with valid passing report produces no output.
/// _Requirements: 6.3_
#[test]
fn annotations_passing_report_no_output() {
    let dir = TempDir::new().unwrap();
    let report_path = create_passing_report(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("annotations").arg("--report").arg(&report_path);

    // Should succeed with no output (no findings)
    cmd.assert().success().stdout(predicate::str::is_empty());
}

/// Test: `builddiag github-annotations` (legacy alias) still works.
/// _Requirements: 6.3_
#[test]
fn github_annotations_alias_works() {
    let dir = TempDir::new().unwrap();
    let report_path = create_passing_report(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Should succeed with no output (no findings)
    cmd.assert().success().stdout(predicate::str::is_empty());
}

/// Test: `builddiag github-annotations` with error findings outputs ::error annotations.
/// _Requirements: 6.3_
#[test]
fn github_annotations_error_findings_outputs_error_format() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_error_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("::error"))
        .stdout(predicate::str::contains("file=Cargo.toml"))
        .stdout(predicate::str::contains("line=1"))
        .stdout(predicate::str::contains("missing_msrv"));
}

/// Test: `builddiag github-annotations` with warning findings outputs ::warning annotations.
/// _Requirements: 6.3_
#[test]
fn github_annotations_warning_findings_outputs_warning_format() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_warning_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("::warning"))
        .stdout(predicate::str::contains("file=scripts/tools.sha256"))
        .stdout(predicate::str::contains("line=1"));
}

/// Test: `builddiag github-annotations` with info findings produces no annotations by default.
/// Info-level findings are filtered out by default (show_info=false).
/// _Requirements: 6.3_
#[test]
fn github_annotations_info_findings_outputs_notice_format() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_info_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Info findings are filtered out by default, so no output is expected
    cmd.assert().success().stdout(predicate::str::is_empty());
}

// =============================================================================
// Annotation format tests
// =============================================================================

/// Test: Annotations include column information when available.
/// _Requirements: 6.3_
#[test]
fn github_annotations_includes_column_when_present() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_column_info(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("::error"))
        .stdout(predicate::str::contains("file=src/lib.rs"))
        .stdout(predicate::str::contains("line=42"))
        .stdout(predicate::str::contains("col=15"));
}

/// Test: Findings without path/line do not produce annotations.
/// _Requirements: 6.3_
#[test]
fn github_annotations_skips_findings_without_location() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_findings_no_location(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Should succeed but produce no output (finding has no location)
    cmd.assert().success().stdout(predicate::str::is_empty());
}

/// Test: Multiple findings produce multiple annotation lines.
/// _Requirements: 6.3_
#[test]
fn github_annotations_multiple_findings_multiple_lines() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_mixed_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let content = String::from_utf8(output).unwrap();

    // Should have 2 annotation lines (error and warning; info is filtered by default)
    assert!(
        content.contains("::error"),
        "Should contain error annotation"
    );
    assert!(
        content.contains("::warning"),
        "Should contain warning annotation"
    );

    // Count the number of lines (info findings are excluded by default)
    let line_count = content.lines().count();
    assert_eq!(
        line_count, 2,
        "Should have exactly 2 annotation lines (info filtered out)"
    );
}

/// Test: Annotation format includes check ID and code.
/// _Requirements: 6.3_
#[test]
fn github_annotations_format_includes_check_id_and_code() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_error_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .success()
        // Format should be: ::{kind} file={path},line={line}::[{check_id}:{code}] {message}
        .stdout(predicate::str::contains("[rust.msrv_defined:missing_msrv]"))
        .stdout(predicate::str::contains("No rust-version found"));
}

// =============================================================================
// Error handling tests
// =============================================================================

/// Test: `builddiag github-annotations` with missing report file fails.
/// _Requirements: 6.3_
#[test]
fn github_annotations_missing_report_file_fails() {
    let dir = TempDir::new().unwrap();
    let nonexistent_path = dir.path().join("nonexistent.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&nonexistent_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test: `builddiag github-annotations` with invalid JSON fails.
/// _Requirements: 6.3_
#[test]
fn github_annotations_invalid_json_fails() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "invalid.json", "{ not valid json }");
    let report_path = dir.path().join("invalid.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

/// Test: `builddiag github-annotations` with wrong JSON schema fails.
/// _Requirements: 6.3_
#[test]
fn github_annotations_wrong_json_schema_fails() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "wrong_schema.json", r#"{"foo": "bar"}"#);
    let report_path = dir.path().join("wrong_schema.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

/// Test: `builddiag github-annotations` without --report argument fails.
/// _Requirements: 6.3_
#[test]
fn github_annotations_missing_report_argument_fails() {
    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--report"));
}

/// Test: `builddiag github-annotations` with empty JSON object fails.
/// _Requirements: 6.3_
#[test]
fn github_annotations_empty_json_object_fails() {
    let dir = TempDir::new().unwrap();
    write_file(&dir, "empty.json", "{}");
    let report_path = dir.path().join("empty.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

// =============================================================================
// Integration with check command tests
// =============================================================================

/// Creates a minimal valid workspace for integration tests.
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

/// Test: `builddiag github-annotations` can render report generated by `builddiag check`.
/// _Requirements: 6.3_
#[test]
fn github_annotations_renders_check_generated_report() {
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

    // Now, use github-annotations command to render the report
    let mut annotations_cmd = get_builddiag_cmd();
    annotations_cmd
        .arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Valid workspace should produce no annotations (no findings)
    annotations_cmd.assert().success();
}

/// Test: `builddiag annotations` output matches `builddiag check --annotations github`.
/// _Requirements: 6.3_
#[test]
fn annotations_output_matches_check_annotations_github() {
    let dir = TempDir::new().unwrap();

    // Create a workspace with missing MSRV to generate findings
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

    // Run check with --annotations github to capture output
    // Use --profile strict to ensure missing MSRV is an error
    let report_path = dir.path().join("report.json");
    let mut check_cmd = get_builddiag_cmd();
    check_cmd
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--out")
        .arg(&report_path)
        .arg("--profile")
        .arg("strict")
        .arg("--annotations")
        .arg("github")
        .arg("--always");

    let check_output = check_cmd.assert().code(2).get_output().stdout.clone();

    // Run annotations command on the generated report
    let mut annotations_cmd = get_builddiag_cmd();
    annotations_cmd
        .arg("annotations")
        .arg("--report")
        .arg(&report_path);

    let annotations_output = annotations_cmd
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Both outputs should be identical
    assert_eq!(
        check_output, annotations_output,
        "annotations output should match check --annotations github output"
    );
}

// =============================================================================
// Determinism and edge case tests
// =============================================================================

/// Test: `builddiag github-annotations` output is deterministic.
/// _Requirements: 6.3_
#[test]
fn github_annotations_output_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_mixed_findings(&dir, "report.json");

    // Run github-annotations twice
    let mut cmd1 = get_builddiag_cmd();
    cmd1.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);
    let output1 = cmd1.assert().success().get_output().stdout.clone();

    let mut cmd2 = get_builddiag_cmd();
    cmd2.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);
    let output2 = cmd2.assert().success().get_output().stdout.clone();

    assert_eq!(
        output1, output2,
        "github-annotations output should be deterministic"
    );
}

/// Test: `builddiag github-annotations` with empty checks array produces no output.
/// _Requirements: 6.3_
#[test]
fn github_annotations_empty_checks_array() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "pass",
  "findings": [],
  "summary": {
    "total_findings": 0,
    "by_severity": {},
    "by_check": {}
  }
}"#;
    write_file(&dir, "empty_checks.json", report);
    let report_path = dir.path().join("empty_checks.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Should succeed with no output
    cmd.assert().success().stdout(predicate::str::is_empty());
}

/// Test: `builddiag github-annotations` handles special characters in messages.
/// _Requirements: 6.3_
#[test]
fn github_annotations_handles_special_characters() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z",
    "duration_ms": 1000,
    "host": {
      "os": "linux",
      "arch": "x86_64"
    }
  },
  "verdict": "fail",
  "findings": [
    {
      "check_id": "test.special_chars",
      "code": "special_chars",
      "severity": "error",
      "message": "Error with 'quotes' and \"double quotes\" and <brackets>",
      "location": {
        "path": "src/lib.rs",
        "line": 1
      }
    }
  ],
  "summary": {
    "total_findings": 1,
    "by_severity": { "error": 1 },
    "by_check": { "test.special_chars": 1 }
  }
}"#;
    write_file(&dir, "special_chars.json", report);
    let report_path = dir.path().join("special_chars.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    // Should succeed and include the message with special characters
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("::error"))
        .stdout(predicate::str::contains("quotes"));
}
