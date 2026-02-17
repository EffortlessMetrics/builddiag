//! Integration tests for baseline workflows.

use assert_cmd::Command;
use builddiag_baseline::Baseline;
use builddiag_types::{Report, Verdict};
use std::fs;
use tempfile::TempDir;

#[allow(deprecated)]
fn get_builddiag_cmd() -> Command {
    Command::cargo_bin("builddiag").unwrap()
}

fn write_file(dir: &TempDir, rel: &str, contents: &str) {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn create_workspace_missing_msrv(dir: &TempDir) {
    write_file(
        dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
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

#[test]
fn baseline_create_writes_default_baseline_file() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_msrv(&dir);

    let mut cmd = get_builddiag_cmd();
    cmd.arg("baseline")
        .arg("create")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict");
    cmd.assert().success().code(0);

    let baseline_path = dir.path().join(".builddiag-baseline.json");
    assert!(baseline_path.exists());

    let txt = fs::read_to_string(&baseline_path).unwrap();
    let baseline: Baseline = serde_json::from_str(&txt).unwrap();
    assert_eq!(baseline.schema, builddiag_baseline::BASELINE_SCHEMA_V1);
    assert!(!baseline.entries.is_empty());
}

#[test]
fn check_with_baseline_reports_only_new_findings() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_msrv(&dir);

    let mut create = get_builddiag_cmd();
    create
        .arg("baseline")
        .arg("create")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict");
    create.assert().success();

    let mut check = get_builddiag_cmd();
    check
        .arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--baseline")
        .arg(".builddiag-baseline.json")
        .arg("--always");
    check.assert().success().code(0);

    let report_path = dir.path().join("artifacts/builddiag/report.json");
    let report_txt = fs::read_to_string(report_path).unwrap();
    let report: Report = serde_json::from_str(&report_txt).unwrap();
    assert_eq!(report.verdict, Verdict::Pass);
    assert!(report.findings.is_empty());

    let baseline_meta = report
        .data
        .as_ref()
        .and_then(|d| d.get("baseline"))
        .expect("baseline metadata should exist");
    assert_eq!(baseline_meta.get("new").and_then(|v| v.as_u64()), Some(0));
    assert!(
        baseline_meta
            .get("suppressed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn baseline_update_merges_new_findings() {
    let dir = TempDir::new().unwrap();
    create_workspace_missing_msrv(&dir);

    let mut create = get_builddiag_cmd();
    create
        .arg("baseline")
        .arg("create")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict");
    create.assert().success();

    let baseline_path = dir.path().join(".builddiag-baseline.json");
    let before: Baseline =
        serde_json::from_str(&fs::read_to_string(&baseline_path).unwrap()).unwrap();
    let before_count = before.entries.len();

    // Introduce a new strict finding not present in baseline.
    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "stable"
"#,
    );

    let mut update = get_builddiag_cmd();
    update
        .arg("baseline")
        .arg("update")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict");
    update.assert().success().code(0);

    let after: Baseline =
        serde_json::from_str(&fs::read_to_string(&baseline_path).unwrap()).unwrap();
    assert!(after.entries.len() > before_count);
}
