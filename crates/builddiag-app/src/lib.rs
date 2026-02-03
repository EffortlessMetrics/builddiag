//! Application orchestration for builddiag build contract validation.
//!
//! This crate provides the high-level orchestration layer that coordinates
//! repository loading, check execution, and report generation. It serves as
//! the bridge between the CLI and the underlying domain logic.
//!
//! # Key Functions
//!
//! - [`load_config`]: Load configuration from a TOML file
//! - [`run_checks`]: Execute all checks and produce a complete report
//! - [`compute_changed_files`]: Determine files changed between git refs for diff-aware mode
//!
//! # Report Generation
//!
//! The main entry point is [`run_checks`], which:
//! 1. Loads repository state from the target directory
//! 2. Runs all configured checks
//! 3. Collects and sorts findings
//! 4. Computes summary statistics
//! 5. Returns a complete [`Report`](builddiag_types::Report)

use anyhow::{Context, Result, anyhow};
use builddiag_checks::run_selected_checks;
use builddiag_domain::{determine_verdict, exit_code_for, sort_findings_canonical, summarize};
use builddiag_render::{render_github_annotations, render_markdown};
use builddiag_repo::load_repo_state;
use builddiag_types::{Config, GitInfo, HostInfo, Report, RunInfo, ToolInfo};
use camino::Utf8Path;
use chrono::Utc;
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

pub const REPORT_SCHEMA_V1: &str = "builddiag.report.v1";

pub fn load_config(path: Option<&Utf8Path>) -> Result<Config> {
    match path {
        None => Ok(Config::default()),
        Some(p) => {
            let txt = fs::read_to_string(p).with_context(|| format!("read config {p}"))?;
            let cfg: Config = toml::from_str(&txt).with_context(|| format!("parse config {p}"))?;
            Ok(cfg)
        }
    }
}

pub fn compute_changed_files(
    root: &Utf8Path,
    base: &str,
    head: &str,
) -> Result<Option<BTreeSet<String>>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-only")
        .arg(format!("{base}...{head}"))
        .output();

    let Ok(out) = out else {
        return Ok(None);
    };
    if !out.status.success() {
        // If git fails (not a repo, no remotes, etc.), fail open.
        return Ok(None);
    }

    let txt = String::from_utf8_lossy(&out.stdout);
    let mut set = BTreeSet::new();
    for line in txt.lines() {
        let p = line.trim();
        if !p.is_empty() {
            set.insert(p.to_string());
        }
    }
    Ok(Some(set))
}

pub struct CheckRun {
    pub report: Report,
    pub markdown: String,
    pub annotations: Vec<String>,
    pub exit_code: i32,
}

pub fn run_check(
    root: &Utf8Path,
    config: &Config,
    allow_all: bool,
    changed_files: Option<BTreeSet<String>>,
) -> Result<CheckRun> {
    let start = Utc::now();

    let repo_state = load_repo_state(root, config, changed_files)?;

    // Run checks and sort findings for deterministic output
    let mut checks = run_selected_checks(&repo_state, config, allow_all)?;
    for check in &mut checks {
        sort_findings_canonical(&mut check.findings);
    }

    // Flatten findings from all check reports
    let mut findings = Vec::new();
    for check in &checks {
        findings.extend(check.findings.clone());
    }
    sort_findings_canonical(&mut findings);

    let summary = summarize(&checks);
    let verdict = determine_verdict(&checks);
    let end = Utc::now();
    let duration_ms = (end - start).num_milliseconds().max(0) as u64;

    // Get git info if available
    let git_info = get_git_info(root);

    let report = Report {
        schema: REPORT_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: RunInfo {
            started_at: start,
            ended_at: Some(end),
            duration_ms,
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            git: git_info,
        },
        verdict,
        findings,
        summary: Some(summary),
    };

    let markdown = render_markdown(&report);
    let annotations = render_github_annotations(&report);
    let exit_code = exit_code_for(verdict, config.defaults.fail_on);

    Ok(CheckRun {
        report,
        markdown,
        annotations,
        exit_code,
    })
}

pub fn write_atomic(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("no parent dir for {path}"))?;
    fs::create_dir_all(parent).with_context(|| format!("create {parent}"))?;

    let tmp = parent.join(format!(".{}.tmp", path.file_name().unwrap_or("out")));
    fs::write(&tmp, bytes).with_context(|| format!("write {tmp}"))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {tmp} -> {path}"))?;
    Ok(())
}

pub fn write_outputs(out_json: &Utf8Path, out_md: Option<&Utf8Path>, run: &CheckRun) -> Result<()> {
    let json = serde_json::to_vec_pretty(&run.report)?;
    write_atomic(out_json, &json)?;

    if let Some(md_path) = out_md {
        write_atomic(md_path, run.markdown.as_bytes())?;
    }

    Ok(())
}

fn get_git_info(root: &Utf8Path) -> Option<GitInfo> {
    // Get current commit SHA
    let commit_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !commit_out.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&commit_out.stdout)
        .trim()
        .to_string();

    // Get current branch name
    let branch_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .ok();
    let branch = branch_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|b| b != "HEAD"); // detached HEAD

    // Check if working directory is dirty
    let status_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .output()
        .ok();
    let dirty = status_out
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    Some(GitInfo {
        commit,
        branch,
        dirty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Tests for load_config function
    /// _Requirements: 2.6, 8.2, 8.3_
    mod load_config_tests {
        use super::*;

        #[test]
        fn load_config_with_none_returns_default_config() {
            let config = load_config(None).expect("should return default config");

            // Verify it's the default config
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Error);
            assert_eq!(config.defaults.out_dir, "artifacts/builddiag");
            assert!(!config.defaults.diff_aware);
            assert!(config.checks.is_empty());
            assert!(config.meta.is_empty());
        }

        #[test]
        fn load_config_with_valid_toml_parses_successfully() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("builddiag.toml");

            let toml_content = r#"
[defaults]
fail_on = "warn"
out_dir = "custom/output"
diff_aware = true
base = "origin/develop"
head = "feature-branch"

[paths]
cargo_root = "custom/Cargo.toml"
rust_toolchain = "custom/rust-toolchain.toml"

[policy.msrv]
require_defined = false
source = "any"

[policy.toolchain]
require_pinned = false
allow_nightly = true

[policy.checksums]
require_file = false

[[checks]]
id = "msrv_defined"
severity = "warn"
enabled = false
triggers = ["Cargo.toml"]

[meta]
custom_key = "custom_value"
"#;

            std::fs::write(&config_path, toml_content).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let config = load_config(Some(utf8_path)).expect("should parse valid TOML");

            // Verify defaults section
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Warn);
            assert_eq!(config.defaults.out_dir, "custom/output");
            assert!(config.defaults.diff_aware);
            assert_eq!(config.defaults.base, "origin/develop");
            assert_eq!(config.defaults.head, "feature-branch");

            // Verify paths section
            assert_eq!(config.paths.cargo_root, "custom/Cargo.toml");
            assert_eq!(config.paths.rust_toolchain, "custom/rust-toolchain.toml");

            // Verify policy section
            assert!(!config.policy.msrv.require_defined);
            assert_eq!(config.policy.msrv.source, builddiag_types::MsrvSource::Any);
            assert!(!config.policy.toolchain.require_pinned);
            assert!(config.policy.toolchain.allow_nightly);
            assert!(!config.policy.checksums.require_file);

            // Verify checks section
            assert_eq!(config.checks.len(), 1);
            let check = &config.checks[0];
            assert_eq!(check.id, "msrv_defined");
            assert_eq!(check.severity, builddiag_types::Severity::Warn);
            assert!(!check.enabled);
            assert_eq!(check.triggers, vec!["Cargo.toml"]);

            // Verify meta section
            assert_eq!(
                config.meta.get("custom_key"),
                Some(&"custom_value".to_string())
            );
        }

        #[test]
        fn load_config_with_minimal_valid_toml() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("minimal.toml");

            // Empty TOML should use all defaults
            let toml_content = "";
            std::fs::write(&config_path, toml_content).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let config = load_config(Some(utf8_path)).expect("should parse empty TOML");

            // Should have all default values
            assert_eq!(config.defaults.fail_on, builddiag_types::FailOn::Error);
            assert_eq!(config.defaults.out_dir, "artifacts/builddiag");
            assert!(config.checks.is_empty());
        }

        #[test]
        fn load_config_with_missing_file_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let nonexistent_path = temp_dir.path().join("nonexistent.toml");

            let utf8_path = Utf8Path::from_path(nonexistent_path.as_path())
                .expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about reading the config file
            assert!(
                err_msg.contains("read config") || err_msg.contains("nonexistent.toml"),
                "Error message should contain context about the missing file: {}",
                err_msg
            );
        }

        #[test]
        fn load_config_with_invalid_toml_returns_error_with_context() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("invalid.toml");

            // Invalid TOML syntax
            let invalid_toml = r#"
[defaults
fail_on = "error"
"#;
            std::fs::write(&config_path, invalid_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(
                err_msg.contains("parse config") || err_msg.contains("invalid.toml"),
                "Error message should contain context about parsing failure: {}",
                err_msg
            );
        }

        #[test]
        fn load_config_with_invalid_enum_value_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("bad_enum.toml");

            // Valid TOML syntax but invalid enum value
            let invalid_enum_toml = r#"
[defaults]
fail_on = "invalid_value"
"#;
            std::fs::write(&config_path, invalid_enum_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(
                err_msg.contains("parse config") || err_msg.contains("bad_enum.toml"),
                "Error message should contain context about parsing failure: {}",
                err_msg
            );
        }

        #[test]
        fn load_config_with_wrong_type_returns_error() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let config_path = temp_dir.path().join("wrong_type.toml");

            // Valid TOML syntax but wrong type (string instead of bool)
            let wrong_type_toml = r#"
[defaults]
diff_aware = "yes"
"#;
            std::fs::write(&config_path, wrong_type_toml).expect("failed to write config file");

            let utf8_path =
                Utf8Path::from_path(config_path.as_path()).expect("path should be valid UTF-8");

            let result = load_config(Some(utf8_path));

            assert!(result.is_err());
            let err = result.unwrap_err();
            let err_msg = format!("{:#}", err);
            // Error should contain context about parsing the config file
            assert!(
                err_msg.contains("parse config") || err_msg.contains("wrong_type.toml"),
                "Error message should contain context about parsing failure: {}",
                err_msg
            );
        }
    }

    /// Tests for write_atomic function
    /// _Requirements: 2.6_
    mod write_atomic_tests {
        use super::*;
        use camino::Utf8PathBuf;

        #[test]
        fn write_atomic_creates_parent_directories() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let nested_path = temp_dir.path().join("nested").join("deep").join("file.txt");
            let utf8_path = Utf8PathBuf::from_path_buf(nested_path.clone())
                .expect("path should be valid UTF-8");

            let content = b"test content";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok(), "write_atomic should succeed: {:?}", result);
            assert!(nested_path.exists(), "file should exist after write");

            let read_content = std::fs::read(&nested_path).expect("should read file");
            assert_eq!(read_content, content, "content should match");
        }

        #[test]
        fn write_atomic_writes_content_correctly() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("output.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"Hello, World!\nThis is a test file.";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok(), "write_atomic should succeed: {:?}", result);

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, content, "content should match exactly");
        }

        #[test]
        fn write_atomic_overwrites_existing_file() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("existing.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Write initial content
            let initial_content = b"initial content";
            std::fs::write(&file_path, initial_content).expect("should write initial file");

            // Overwrite with new content using write_atomic
            let new_content = b"new content that is different";
            let result = write_atomic(&utf8_path, new_content);

            assert!(result.is_ok(), "write_atomic should succeed: {:?}", result);

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, new_content, "content should be overwritten");
        }

        #[test]
        fn write_atomic_handles_empty_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("empty.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"";
            let result = write_atomic(&utf8_path, content);

            assert!(
                result.is_ok(),
                "write_atomic should succeed with empty content: {:?}",
                result
            );

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert!(read_content.is_empty(), "file should be empty");
        }

        #[test]
        fn write_atomic_handles_binary_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("binary.bin");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Binary content with null bytes and various byte values
            let content: Vec<u8> = (0u8..=255).collect();
            let result = write_atomic(&utf8_path, &content);

            assert!(
                result.is_ok(),
                "write_atomic should succeed with binary content: {:?}",
                result
            );

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(read_content, content, "binary content should match exactly");
        }

        #[test]
        fn write_atomic_cleans_up_temp_file_on_success() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("cleanup_test.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = b"test content";
            let result = write_atomic(&utf8_path, content);

            assert!(result.is_ok(), "write_atomic should succeed: {:?}", result);

            // Check that no .tmp file remains in the directory
            let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
                .expect("should read dir")
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
                .collect();

            assert!(
                entries.is_empty(),
                "no .tmp files should remain after successful write"
            );
        }

        #[test]
        fn write_atomic_returns_error_for_root_path() {
            // A path with no parent (like "/" on Unix) should return an error
            let root_path = Utf8Path::new("/");
            let content = b"test";

            let result = write_atomic(root_path, content);

            assert!(result.is_err(), "write_atomic should fail for root path");
            let err_msg = format!("{:#}", result.unwrap_err());
            assert!(
                err_msg.contains("no parent dir"),
                "error should mention no parent dir: {}",
                err_msg
            );
        }

        #[test]
        fn write_atomic_handles_large_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("large.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            // Create 1MB of content
            let content: Vec<u8> = vec![b'x'; 1024 * 1024];
            let result = write_atomic(&utf8_path, &content);

            assert!(
                result.is_ok(),
                "write_atomic should succeed with large content: {:?}",
                result
            );

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(
                read_content.len(),
                content.len(),
                "large content size should match"
            );
            assert_eq!(read_content, content, "large content should match exactly");
        }

        #[test]
        fn write_atomic_preserves_unicode_in_content() {
            let temp_dir = TempDir::new().expect("failed to create temp dir");
            let file_path = temp_dir.path().join("unicode.txt");
            let utf8_path =
                Utf8PathBuf::from_path_buf(file_path.clone()).expect("path should be valid UTF-8");

            let content = "Hello, 世界! 🦀 Rust is awesome! Ñoño".as_bytes();
            let result = write_atomic(&utf8_path, content);

            assert!(
                result.is_ok(),
                "write_atomic should succeed with unicode content: {:?}",
                result
            );

            let read_content = std::fs::read(&file_path).expect("should read file");
            assert_eq!(
                read_content, content,
                "unicode content should match exactly"
            );
        }
    }
}
