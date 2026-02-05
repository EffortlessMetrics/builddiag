use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use schemars::schema_for;

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate JSON Schemas into `schemas/`.
    Schema {
        #[arg(long, default_value = "schemas")]
        out_dir: Utf8PathBuf,
    },
    /// Run common CI steps.
    Ci,
    /// Generate code coverage reports using cargo-llvm-cov.
    ///
    /// This command requires cargo-llvm-cov to be installed:
    ///   cargo install cargo-llvm-cov
    ///
    /// Or via rustup:
    ///   rustup component add llvm-tools-preview
    ///   cargo install cargo-llvm-cov
    ///
    /// Coverage reports are generated in the `coverage/` directory:
    ///   - coverage/lcov.info: LCOV format for CI integration (codecov, coveralls)
    ///   - coverage/html/: HTML report for local viewing
    Coverage {
        /// Output directory for coverage reports
        #[arg(long, default_value = "coverage")]
        out_dir: Utf8PathBuf,
        /// Generate HTML report in addition to LCOV
        #[arg(long)]
        html: bool,
        /// Open HTML report in browser after generation
        #[arg(long)]
        open: bool,
    },
    /// Run conformance tests for Cockpit CI governance compliance.
    ///
    /// Validates that builddiag output conforms to sensor.report.v1 schema
    /// and produces deterministic output.
    Conform {
        /// Directory containing conformance fixtures
        #[arg(long, default_value = "fixtures/conformance")]
        fixtures: Utf8PathBuf,
        /// Directory containing golden files
        #[arg(long, default_value = "fixtures/golden")]
        golden: Utf8PathBuf,
        /// Update golden files instead of comparing
        #[arg(long)]
        update_golden: bool,
        /// Run only specific tests (schema, determinism, survivability)
        #[arg(long)]
        only: Option<Vec<String>>,
    },
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("xtask: {e:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Schema { out_dir } => generate_schemas(&out_dir),
        Command::Ci => run_ci(),
        Command::Coverage {
            out_dir,
            html,
            open,
        } => run_coverage(&out_dir, html, open),
        Command::Conform {
            fixtures,
            golden,
            update_golden,
            only,
        } => run_conform(&fixtures, &golden, update_golden, only),
    }
}

fn generate_schemas(out_dir: &Utf8Path) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;

    // Native builddiag schemas only.
    // sensor.report.v1.schema.json is owned by the contracts pack, not generated here.
    let report = schema_for!(builddiag_types::Report);
    let cfg = schema_for!(builddiag_types::Config);

    write_json(out_dir.join("builddiag.report.v1.schema.json"), &report)?;
    write_json(out_dir.join("builddiag.config.v1.schema.json"), &cfg)?;

    println!("Generated schemas:");
    println!("  - {}/builddiag.report.v1.schema.json", out_dir);
    println!("  - {}/builddiag.config.v1.schema.json", out_dir);

    Ok(())
}

fn write_json(path: Utf8PathBuf, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(&path, bytes).with_context(|| format!("write {path}"))?;
    Ok(())
}

// =============================================================================
// Conformance Testing
// =============================================================================

fn run_conform(
    fixtures: &Utf8Path,
    golden: &Utf8Path,
    update_golden: bool,
    only: Option<Vec<String>>,
) -> Result<()> {
    println!("Running conformance tests...\n");

    let should_run = |name: &str| only.as_ref().is_none_or(|v| v.iter().any(|s| s == name));

    let mut passed = 0;
    let mut failed = 0;

    // 1. Schema Validation
    if should_run("schema") {
        println!("=== Schema Validation ===");
        match run_schema_validation(fixtures) {
            Ok(()) => {
                println!("  PASS: All reports validate against sensor.report.v1 schema\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Schema validation failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 2. Determinism Check
    if should_run("determinism") {
        println!("=== Determinism Check ===");
        match run_determinism_check(fixtures) {
            Ok(()) => {
                println!("  PASS: Output is deterministic (ignoring timestamps)\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Determinism check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 3. Survivability Check
    if should_run("survivability") {
        println!("=== Survivability Check ===");
        match run_survivability_check(fixtures) {
            Ok(()) => {
                println!("  PASS: Error receipt is valid JSON on config error\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Survivability check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 4. Artifact Layout Check
    if should_run("layout") {
        println!("=== Artifact Layout Check ===");
        match run_layout_check(fixtures) {
            Ok(()) => {
                println!("  PASS: Artifact layout is correct\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Artifact layout check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 5. Golden File Check (optional)
    if should_run("golden") {
        println!("=== Golden File Check ===");
        match run_golden_check(fixtures, golden, update_golden) {
            Ok(()) => {
                if update_golden {
                    println!("  INFO: Golden files updated\n");
                } else {
                    println!("  PASS: Output matches golden files (excluding timestamps)\n");
                }
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Golden file check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // Summary
    println!("=== Summary ===");
    println!("  Passed: {passed}");
    println!("  Failed: {failed}");

    if failed > 0 {
        anyhow::bail!("{failed} conformance test(s) failed");
    }

    Ok(())
}

fn run_schema_validation(fixtures: &Utf8Path) -> Result<()> {
    // Load the sensor schema from the contracts pack (shared ABI, not locally generated)
    let schema_path = Utf8Path::new("contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = std::fs::read_to_string(schema_path)
        .with_context(|| format!("read schema from {schema_path}"))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_content).context("parse schema JSON")?;

    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| anyhow::anyhow!("compile schema: {e}"))?;

    // Test valid-workspace fixture
    let valid_dir = fixtures.join("valid-workspace");
    if valid_dir.exists() {
        let report = run_builddiag_sensor(&valid_dir)?;
        let report_value: serde_json::Value =
            serde_json::from_str(&report).context("parse valid-workspace report")?;

        if let Err(e) = validator.validate(&report_value) {
            anyhow::bail!("valid-workspace report failed schema validation: {}", e);
        }
        println!("    valid-workspace: OK");
    }

    // Test missing-msrv fixture
    let missing_dir = fixtures.join("missing-msrv");
    if missing_dir.exists() {
        let report = run_builddiag_sensor(&missing_dir)?;
        let report_value: serde_json::Value =
            serde_json::from_str(&report).context("parse missing-msrv report")?;

        if let Err(e) = validator.validate(&report_value) {
            anyhow::bail!("missing-msrv report failed schema validation: {}", e);
        }
        println!("    missing-msrv: OK");
    }

    Ok(())
}

fn run_determinism_check(fixtures: &Utf8Path) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    if !valid_dir.exists() {
        return Ok(()); // Skip if no fixture
    }

    // Run twice and compare (after normalizing timestamps)
    let report1 = run_builddiag_sensor(&valid_dir)?;
    let report2 = run_builddiag_sensor(&valid_dir)?;

    let normalized1 = normalize_for_comparison(&report1)?;
    let normalized2 = normalize_for_comparison(&report2)?;

    if normalized1 != normalized2 {
        anyhow::bail!(
            "Non-deterministic output detected!\nRun 1:\n{}\n\nRun 2:\n{}",
            normalized1,
            normalized2
        );
    }

    println!("    Two runs produced identical output (after normalizing timestamps)");
    Ok(())
}

fn run_survivability_check(fixtures: &Utf8Path) -> Result<()> {
    let broken_dir = fixtures.join("broken-config");
    if !broken_dir.exists() {
        return Ok(()); // Skip if no fixture
    }

    // Create temp file for output
    let temp_dir = tempfile::TempDir::new()?;
    let out_file = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("error-receipt.json");

    // Run builddiag in cockpit mode with broken config
    let config_path = broken_dir.join(".builddiag.toml");
    let status = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--root",
            broken_dir.as_str(),
            "--config",
            config_path.as_str(),
            "--format",
            "sensor",
            "--mode",
            "cockpit",
            "--out",
            out_file.as_str(),
        ])
        .output()
        .context("run builddiag")?;

    // In cockpit mode, exit code should be 0 (report written) or 1 (catastrophic)
    // Since config is broken, we expect the error receipt to be written
    if !out_file.exists() {
        anyhow::bail!("Error receipt was not written to {out_file}");
    }

    // Verify it's valid JSON
    let receipt_content = std::fs::read_to_string(&out_file)?;
    let _: serde_json::Value =
        serde_json::from_str(&receipt_content).context("error receipt is not valid JSON")?;

    println!(
        "    Broken config produced valid JSON error receipt (exit code: {})",
        status.status.code().unwrap_or(-1)
    );
    Ok(())
}

fn run_layout_check(fixtures: &Utf8Path) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    if !valid_dir.exists() {
        return Ok(());
    }

    let temp_dir = tempfile::TempDir::new()?;
    let artifacts_dir = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("artifacts/builddiag");

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--root",
            valid_dir.as_str(),
            "--profile",
            "oss",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ])
        .output()
        .context("run builddiag with --artifacts-dir")?;

    // Verify report.json exists and is sensor format
    let report_path = artifacts_dir.join("report.json");
    if !report_path.exists() {
        anyhow::bail!("report.json not found at {report_path}");
    }
    let report_content = std::fs::read_to_string(&report_path)?;
    let report: serde_json::Value =
        serde_json::from_str(&report_content).context("parse report.json")?;
    let schema = report
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if schema != "sensor.report.v1" {
        anyhow::bail!(
            "report.json has schema '{schema}', expected 'sensor.report.v1'"
        );
    }
    println!("    report.json: OK (sensor.report.v1)");

    // Verify comment.md exists
    let comment_path = artifacts_dir.join("comment.md");
    if !comment_path.exists() {
        anyhow::bail!("comment.md not found at {comment_path}");
    }
    println!("    comment.md: OK");

    // Verify extras/payload.json exists and is builddiag format
    let payload_path = artifacts_dir.join("extras/payload.json");
    if !payload_path.exists() {
        anyhow::bail!("extras/payload.json not found at {payload_path}");
    }
    let payload_content = std::fs::read_to_string(&payload_path)?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_content).context("parse extras/payload.json")?;
    let payload_schema = payload
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if payload_schema != "builddiag.report.v1" {
        anyhow::bail!(
            "extras/payload.json has schema '{payload_schema}', expected 'builddiag.report.v1'"
        );
    }
    println!("    extras/payload.json: OK (builddiag.report.v1)");

    // Verify no path traversal or absolute paths in report artifacts
    if let Some(artifacts) = report.get("artifacts").and_then(|v| v.as_array()) {
        for art in artifacts {
            if let Some(path) = art.get("path").and_then(|v| v.as_str()) {
                if path.contains("..") {
                    anyhow::bail!("artifact path contains '..': {path}");
                }
                if path.starts_with('/') || (path.len() > 1 && path.as_bytes()[1] == b':') {
                    anyhow::bail!("artifact path is absolute: {path}");
                }
            }
        }
    }
    println!("    artifact paths: OK (no traversal, no absolute)");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("    note: exit code {} (stderr: {stderr})", output.status.code().unwrap_or(-1));
    }

    Ok(())
}

fn run_golden_check(fixtures: &Utf8Path, golden: &Utf8Path, update: bool) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    let valid_golden = golden.join("valid-workspace.report.json");

    if !valid_dir.exists() {
        return Ok(());
    }

    let current = run_builddiag_sensor(&valid_dir)?;
    let current_normalized = normalize_for_comparison(&current)?;

    if update {
        std::fs::create_dir_all(golden)?;
        std::fs::write(&valid_golden, &current)?;
        println!("    Updated {valid_golden}");
    } else if valid_golden.exists() {
        let golden_content = std::fs::read_to_string(&valid_golden)?;
        let golden_normalized = normalize_for_comparison(&golden_content)?;

        if current_normalized != golden_normalized {
            anyhow::bail!(
                "Output differs from golden file {valid_golden}\n\nExpected:\n{}\n\nGot:\n{}",
                golden_normalized,
                current_normalized
            );
        }
        println!("    valid-workspace.report.json: OK");
    }

    Ok(())
}

fn run_builddiag_sensor(root: &Utf8Path) -> Result<String> {
    let temp_dir = tempfile::TempDir::new()?;
    let out_file = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("report.json");

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--root",
            root.as_str(),
            "--format",
            "sensor",
            "--profile",
            "oss",
            "--out",
            out_file.as_str(),
        ])
        .output()
        .context("run builddiag")?;

    // builddiag might exit with 2 for policy violations, which is ok
    if !out_file.exists() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("builddiag did not produce output: {stderr}");
    }

    std::fs::read_to_string(&out_file).context("read builddiag output")
}

fn normalize_for_comparison(json: &str) -> Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;

    // Remove timestamps and duration for determinism
    if let Some(obj) = value.as_object_mut()
        && let Some(run) = obj.get_mut("run").and_then(|r| r.as_object_mut())
    {
        run.remove("started_at");
        run.remove("ended_at");
        run.remove("duration_ms");

        // Also normalize git info (commit might change)
        if let Some(git) = run.get_mut("git").and_then(|g| g.as_object_mut()) {
            git.insert(
                "commit".to_string(),
                serde_json::Value::String("NORMALIZED".to_string()),
            );
            git.insert("dirty".to_string(), serde_json::Value::Bool(false));
        }
    }

    serde_json::to_string_pretty(&value).context("serialize normalized JSON")
}

fn run_ci() -> Result<()> {
    // Use a separate target directory to avoid locking issues on Windows.
    // When xtask runs `cargo run -p xtask`, it would try to overwrite the
    // running xtask.exe, which fails on Windows. Using target/ci sidesteps this.
    let target_dir = Some("target/ci");

    run_with_target_dir(["cargo", "fmt", "--all", "--", "--check"], target_dir)?;
    run_with_target_dir(
        [
            "cargo",
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        target_dir,
    )?;
    run_with_target_dir(["cargo", "test", "--all"], target_dir)?;
    run_with_target_dir(["cargo", "run", "-p", "xtask", "--", "schema"], target_dir)?;
    run_with_target_dir(["cargo", "run", "-p", "xtask", "--", "conform"], target_dir)?;
    Ok(())
}

fn run_with_target_dir(
    args: impl IntoIterator<Item = &'static str>,
    target_dir: Option<&str>,
) -> Result<()> {
    let mut it = args.into_iter();
    let cmd = it.next().unwrap();
    let mut command = std::process::Command::new(cmd);
    command.args(it);
    if let Some(dir) = target_dir {
        command.env("CARGO_TARGET_DIR", dir);
    }
    let status = command.status().with_context(|| format!("run {cmd}"))?;
    if !status.success() {
        anyhow::bail!("command failed: {cmd}");
    }
    Ok(())
}

/// Run code coverage using cargo-llvm-cov.
///
/// # Prerequisites
///
/// Install cargo-llvm-cov:
/// ```bash
/// rustup component add llvm-tools-preview
/// cargo install cargo-llvm-cov
/// ```
///
/// # Usage
///
/// Generate LCOV report only (for CI):
/// ```bash
/// cargo run -p xtask -- coverage
/// ```
///
/// Generate HTML report for local viewing:
/// ```bash
/// cargo run -p xtask -- coverage --html
/// ```
///
/// Generate and open HTML report:
/// ```bash
/// cargo run -p xtask -- coverage --html --open
/// ```
///
/// # Output
///
/// - `coverage/lcov.info`: LCOV format for codecov/coveralls integration
/// - `coverage/html/index.html`: HTML report (when --html is specified)
///
/// # Coverage Expectations
///
/// Based on the comprehensive test coverage spec:
/// - Target line coverage: 80%
/// - Target branch coverage: 70%
///
/// The coverage report shows per-crate breakdown:
/// - builddiag-types: Config, Report, Finding types
/// - builddiag-domain: Version parsing, summarization
/// - builddiag-repo: File parsing, workspace loading
/// - builddiag-checks: All check implementations
/// - builddiag-render: Markdown and annotation rendering
/// - builddiag-app: Config loading, orchestration
/// - builddiag-cli: CLI entry point
fn run_coverage(out_dir: &Utf8Path, html: bool, open: bool) -> Result<()> {
    // Check if cargo-llvm-cov is installed
    let check = std::process::Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .output();

    match check {
        Ok(output) if output.status.success() => {}
        _ => {
            anyhow::bail!(
                "cargo-llvm-cov is not installed.\n\
                 Install it with:\n  \
                 rustup component add llvm-tools-preview\n  \
                 cargo install cargo-llvm-cov"
            );
        }
    }

    // Create output directory
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;

    // Generate LCOV report
    let lcov_path = out_dir.join("lcov.info");
    println!("Generating LCOV coverage report: {lcov_path}");

    let status = std::process::Command::new("cargo")
        .args([
            "llvm-cov",
            "--all-features",
            "--workspace",
            "--lcov",
            "--output-path",
            lcov_path.as_str(),
        ])
        .status()
        .context("run cargo llvm-cov")?;

    if !status.success() {
        anyhow::bail!("cargo llvm-cov failed");
    }

    println!("LCOV report generated: {lcov_path}");

    // Generate HTML report if requested
    if html {
        let html_dir = out_dir.join("html");
        println!("Generating HTML coverage report: {html_dir}");

        let status = std::process::Command::new("cargo")
            .args([
                "llvm-cov",
                "--all-features",
                "--workspace",
                "--html",
                "--output-dir",
                html_dir.as_str(),
            ])
            .status()
            .context("run cargo llvm-cov --html")?;

        if !status.success() {
            anyhow::bail!("cargo llvm-cov --html failed");
        }

        let index_path = html_dir.join("index.html");
        println!("HTML report generated: {index_path}");

        // Open in browser if requested
        if open {
            println!("Opening coverage report in browser...");
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .arg(index_path.as_str())
                .status();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open")
                .arg(index_path.as_str())
                .status();
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", index_path.as_str()])
                .status();
        }
    }

    // Print summary
    println!("\n=== Coverage Summary ===");
    println!("LCOV report: {lcov_path}");
    if html {
        println!("HTML report: {}/html/index.html", out_dir);
    }
    println!("\nTo upload to codecov:");
    println!("  codecov -f {lcov_path}");
    println!("\nCoverage targets (from spec):");
    println!("  - Line coverage: 80%");
    println!("  - Branch coverage: 70%");

    Ok(())
}
