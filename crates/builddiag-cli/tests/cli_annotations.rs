//! Integration tests for the `builddiag github-annotations` command.
//!
//! These tests validate the CLI behavior for rendering GitHub Actions annotations
//! from JSON reports.
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

/// Creates a JSON report with error findings that have path and line (for annotations).
fn create_report_with_error_findings(dir: &TempDir, rel: &str) -> String {
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
    "rust_toolchain": null,
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

/// Creates a JSON report with warning findings.
fn create_report_with_warning_findings(dir: &TempDir, rel: &str) -> String {
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
    "rust_toolchain": "rust-toolchain.toml",
    "tools_checksums": null,
    "tools_manifest": null
  },
  "checks": [
    {
      "id": "checksums.file_exists",
      "status": "warn",
      "findings": [
        {
          "severity": "warn",
          "code": "missing_checksums",
          "message": "Checksums file not found",
          "path": "scripts/tools.sha256",
          "line": 1,
          "column": null
        }
      ],
      "skipped_reason": null
    }
  ],
  "summary": {
    "counts": {
      "info": 0,
      "warn": 1,
      "error": 0
    },
    "verdict": "warn",
    "reasons": ["1 warning found"]
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

/// Creates a JSON report with info findings.
fn create_report_with_info_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-004",
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
      "id": "info.check",
      "status": "pass",
      "findings": [
        {
          "severity": "info",
          "code": "info_code",
          "message": "Informational message",
          "path": "README.md",
          "line": 10,
          "column": null
        }
      ],
      "skipped_reason": null
    }
  ],
  "summary": {
    "counts": {
      "info": 1,
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

/// Creates a JSON report with findings that have column information.
fn create_report_with_column_info(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-005",
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
  "checks": [
    {
      "id": "syntax.error",
      "status": "fail",
      "findings": [
        {
          "severity": "error",
          "code": "syntax_error",
          "message": "Syntax error at position",
          "path": "src/lib.rs",
          "line": 42,
          "column": 15
        }
      ],
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

/// Creates a JSON report with findings that lack path/line (should not produce annotations).
fn create_report_with_findings_no_location(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-006",
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
  "checks": [
    {
      "id": "general.check",
      "status": "fail",
      "findings": [
        {
          "severity": "error",
          "code": "general_error",
          "message": "General error without location",
          "path": null,
          "line": null,
          "column": null
        }
      ],
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

/// Creates a JSON report with multiple findings of different severities.
fn create_report_with_mixed_findings(dir: &TempDir, rel: &str) -> String {
    let report = r#"{
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-007",
    "started_at": "2024-01-01T00:00:00Z",
    "ended_at": "2024-01-01T00:00:01Z"
  },
  "repo": {
    "root": "/test/repo",
    "detected": {
      "is_workspace": true,
      "members": 2
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
          "message": "No rust-version found",
          "path": "Cargo.toml",
          "line": 1,
          "column": null
        }
      ],
      "skipped_reason": null
    },
    {
      "id": "toolchain.pinned",
      "status": "warn",
      "findings": [
        {
          "severity": "warn",
          "code": "unpinned_toolchain",
          "message": "Toolchain is not pinned to a specific version",
          "path": "rust-toolchain.toml",
          "line": 2,
          "column": null
        }
      ],
      "skipped_reason": null
    },
    {
      "id": "info.check",
      "status": "pass",
      "findings": [
        {
          "severity": "info",
          "code": "info_note",
          "message": "Consider adding documentation",
          "path": "src/lib.rs",
          "line": 5,
          "column": null
        }
      ],
      "skipped_reason": null
    }
  ],
  "summary": {
    "counts": {
      "info": 1,
      "warn": 1,
      "error": 1
    },
    "verdict": "fail",
    "reasons": ["1 error, 1 warning found"]
  }
}"#;
    write_file(dir, rel, report);
    dir.path().join(rel).to_string_lossy().to_string()
}

// =============================================================================
// Basic github-annotations command tests
// =============================================================================

/// Test: `builddiag github-annotations` with valid passing report produces no output.
/// _Requirements: 6.3_
#[test]
fn github_annotations_passing_report_no_output() {
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

/// Test: `builddiag github-annotations` with info findings outputs ::notice annotations.
/// _Requirements: 6.3_
#[test]
fn github_annotations_info_findings_outputs_notice_format() {
    let dir = TempDir::new().unwrap();
    let report_path = create_report_with_info_findings(&dir, "report.json");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("github-annotations")
        .arg("--report")
        .arg(&report_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("::notice"))
        .stdout(predicate::str::contains("file=README.md"))
        .stdout(predicate::str::contains("line=10"));
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

    // Should have 3 annotation lines (error, warning, notice)
    assert!(
        content.contains("::error"),
        "Should contain error annotation"
    );
    assert!(
        content.contains("::warning"),
        "Should contain warning annotation"
    );
    assert!(
        content.contains("::notice"),
        "Should contain notice annotation"
    );

    // Count the number of lines
    let line_count = content.lines().count();
    assert_eq!(line_count, 3, "Should have exactly 3 annotation lines");
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
        .stdout(predicate::str::contains(
            "[msrv.workspace_msrv_defined:missing_msrv]",
        ))
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

/// Test: `builddiag github-annotations` output matches `builddiag check --github-annotations`.
/// _Requirements: 6.3_
#[test]
fn github_annotations_output_matches_check_github_annotations() {
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

    // Run check with --github-annotations to capture output
    let report_path = dir.path().join("report.json");
    let mut check_cmd = get_builddiag_cmd();
    check_cmd
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--out")
        .arg(&report_path)
        .arg("--github-annotations")
        .arg("--always");

    let check_output = check_cmd.assert().code(2).get_output().stdout.clone();

    // Run github-annotations command on the generated report
    let mut annotations_cmd = get_builddiag_cmd();
    annotations_cmd
        .arg("github-annotations")
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
        "github-annotations output should match check --github-annotations output"
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
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-008",
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
  "schema": "builddiag/v1",
  "tool": {
    "name": "builddiag",
    "version": "0.1.0"
  },
  "run": {
    "id": "test-run-009",
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
  "checks": [
    {
      "id": "test.special_chars",
      "status": "fail",
      "findings": [
        {
          "severity": "error",
          "code": "special_chars",
          "message": "Error with 'quotes' and \"double quotes\" and <brackets>",
          "path": "src/lib.rs",
          "line": 1,
          "column": null
        }
      ],
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
