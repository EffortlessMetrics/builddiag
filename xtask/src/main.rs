use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use schemars::schema_for;
#[cfg(test)]
use std::sync::Mutex;

#[derive(Debug, Clone)]
struct Cmd {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl Cmd {
    fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

trait CommandRunner {
    fn status(&self, cmd: &Cmd) -> Result<std::process::ExitStatus>;
    fn output(&self, cmd: &Cmd) -> Result<std::process::Output>;
}

struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn status(&self, cmd: &Cmd) -> Result<std::process::ExitStatus> {
        let mut command = std::process::Command::new(&cmd.program);
        command.args(&cmd.args);
        for (k, v) in &cmd.env {
            command.env(k, v);
        }
        Ok(command.status()?)
    }

    fn output(&self, cmd: &Cmd) -> Result<std::process::Output> {
        let mut command = std::process::Command::new(&cmd.program);
        command.args(&cmd.args);
        for (k, v) in &cmd.env {
            command.env(k, v);
        }
        Ok(command.output()?)
    }
}

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

#[cfg(test)]
static MAIN_ARGS: Mutex<Option<Vec<std::ffi::OsString>>> = Mutex::new(None);

fn main() -> std::process::ExitCode {
    let cli = Cli::parse_from(main_args());
    let runner = RealCommandRunner;
    let code = run_main(cli, &runner);
    std::process::ExitCode::from(code as u8)
}

fn main_args() -> Vec<std::ffi::OsString> {
    #[cfg(test)]
    {
        if let Some(args) = MAIN_ARGS.lock().unwrap().take() {
            return args;
        }
    }
    std::env::args_os().collect()
}

fn run_cli(cli: Cli, runner: &dyn CommandRunner) -> Result<()> {
    match cli.cmd {
        Command::Schema { out_dir } => generate_schemas(&out_dir),
        Command::Ci => run_ci(runner),
        Command::Coverage {
            out_dir,
            html,
            open,
        } => run_coverage(runner, &out_dir, html, open),
        Command::Conform {
            fixtures,
            golden,
            update_golden,
            only,
        } => run_conform(runner, &fixtures, &golden, update_golden, only),
    }
}

fn run_main(cli: Cli, runner: &dyn CommandRunner) -> i32 {
    match run_cli(cli, runner) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("xtask: {e:#}");
            1
        }
    }
}

#[cfg(test)]
fn try_main_from<I>(args: I, runner: &dyn CommandRunner) -> Result<()>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let cli = Cli::try_parse_from(args)?;
    run_cli(cli, runner)
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

fn workspace_root() -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .join("..")
}

// =============================================================================
// Conformance Testing
// =============================================================================

fn run_conform(
    runner: &dyn CommandRunner,
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
        match run_schema_validation(runner, fixtures) {
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
        match run_determinism_check(runner, fixtures) {
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
        match run_survivability_check(runner, fixtures) {
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
        match run_layout_check(runner, fixtures) {
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
        match run_golden_check(runner, fixtures, golden, update_golden) {
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

    // 6. Tool Error Convention Check
    if should_run("tool-error") {
        println!("=== Tool Error Convention Check ===");
        match run_tool_error_check(runner, fixtures) {
            Ok(()) => {
                println!("  PASS: Tool error produces correct receipt\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Tool error check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 7. Library Parity Check
    if should_run("library-parity") {
        println!("=== Library Parity Check ===");
        match run_library_parity_check(runner, fixtures) {
            Ok(()) => {
                println!("  PASS: Library and CLI produce identical sensor reports\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Library parity check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 8. Native Schema Validation
    if should_run("native-schema") {
        println!("=== Native Schema Validation ===");
        match run_native_schema_validation(runner, fixtures) {
            Ok(()) => {
                println!(
                    "  PASS: All native reports validate against builddiag.report.v1 schema\n"
                );
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Native schema validation failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 9. Verdict Contract Check
    if should_run("verdict-contract") {
        println!("=== Verdict Contract Check ===");
        match run_verdict_contract_check(runner, fixtures) {
            Ok(()) => {
                println!("  PASS: Verdict contract is consistent across all fixtures\n");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Verdict contract check failed: {e:#}\n");
                failed += 1;
            }
        }
    }

    // 10. Native Golden File Check
    if should_run("native-golden") {
        println!("=== Native Golden File Check ===");
        match run_native_golden_check(runner, fixtures, golden, update_golden) {
            Ok(()) => {
                if update_golden {
                    println!("  INFO: Native golden files updated\n");
                } else {
                    println!("  PASS: Native output matches golden files (excluding timestamps)\n");
                }
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: Native golden file check failed: {e:#}\n");
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

fn run_schema_validation(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    // Load the sensor schema from the contracts pack (shared ABI, not locally generated)
    let schema_path = workspace_root().join("contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = std::fs::read_to_string(&schema_path)
        .with_context(|| format!("read schema from {schema_path}"))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_content).context("parse schema JSON")?;

    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| anyhow::anyhow!("compile schema: {e}"))?;

    // Auto-discover fixture directories (skip broken-config and tool-error
    // which are special cases that don't produce sensor reports)
    let skip_fixtures = ["broken-config", "tool-error"];
    for entry in discover_fixture_dirs(fixtures)? {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name for fixture dir"))?;
        if skip_fixtures.contains(&name) {
            continue;
        }

        let report = run_builddiag_sensor(runner, &entry)?;
        let report_value: serde_json::Value =
            serde_json::from_str(&report).with_context(|| format!("parse {name} report"))?;

        if let Err(e) = validator.validate(&report_value) {
            anyhow::bail!("{name} report failed schema validation: {}", e);
        }
        println!("    {name}: OK");
    }

    Ok(())
}

fn run_determinism_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    if !valid_dir.exists() {
        return Ok(()); // Skip if no fixture
    }

    // Run twice and compare (after normalizing timestamps)
    let report1 = run_builddiag_sensor(runner, &valid_dir)?;
    let report2 = run_builddiag_sensor(runner, &valid_dir)?;

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

fn run_survivability_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
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
    let status = runner
        .output(&Cmd::new("cargo").args([
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
        ]))
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

fn run_layout_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    if !valid_dir.exists() {
        return Ok(());
    }

    let temp_dir = tempfile::TempDir::new()?;
    let artifacts_dir = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("artifacts/builddiag");

    let output = runner
        .output(&Cmd::new("cargo").args([
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
        ]))
        .context("run builddiag with --artifacts-dir")?;

    // Verify report.json exists and is sensor format
    let report_path = artifacts_dir.join("report.json");
    if !report_path.exists() {
        anyhow::bail!("report.json not found at {report_path}");
    }
    let report_content = std::fs::read_to_string(&report_path)?;
    let report: serde_json::Value =
        serde_json::from_str(&report_content).context("parse report.json")?;
    let schema = report.get("schema").and_then(|v| v.as_str()).unwrap_or("");
    if schema != "sensor.report.v1" {
        anyhow::bail!("report.json has schema '{schema}', expected 'sensor.report.v1'");
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
    let payload_schema = payload.get("schema").and_then(|v| v.as_str()).unwrap_or("");
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
    // Verify artifacts array includes both "payload" and "comment"
    if let Some(artifacts) = report.get("artifacts").and_then(|v| v.as_array()) {
        let has_payload = artifacts
            .iter()
            .any(|a| a.get("name").and_then(|v| v.as_str()) == Some("payload"));
        let has_comment = artifacts
            .iter()
            .any(|a| a.get("name").and_then(|v| v.as_str()) == Some("comment"));
        if !has_payload {
            anyhow::bail!("artifacts array missing 'payload' entry");
        }
        if !has_comment {
            anyhow::bail!("artifacts array missing 'comment' entry");
        }
        println!("    artifacts: OK (payload + comment declared)");
    } else {
        anyhow::bail!("report.json missing 'artifacts' array");
    }
    println!("    artifact paths: OK (no traversal, no absolute)");

    // Verify finding location paths use forward slashes and are relative
    if let Some(findings) = report.get("findings").and_then(|v| v.as_array()) {
        for finding in findings {
            if let Some(loc) = finding.get("location")
                && let Some(path) = loc.get("path").and_then(|v| v.as_str())
            {
                if path.contains('\\') {
                    anyhow::bail!("finding location path contains backslash: {path}");
                }
                if path.contains("..") {
                    anyhow::bail!("finding location path contains traversal: {path}");
                }
                if path.starts_with('/')
                    || (path.len() > 1 && path.as_bytes()[1] == b':')
                    || path.starts_with("\\\\")
                {
                    anyhow::bail!("finding location path is absolute: {path}");
                }
            }
        }
    }
    println!("    finding location paths: OK (no traversal, no absolute, forward slashes)");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!(
            "    note: exit code {} (stderr: {stderr})",
            output.status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

fn run_golden_check(
    runner: &dyn CommandRunner,
    fixtures: &Utf8Path,
    golden: &Utf8Path,
    update: bool,
) -> Result<()> {
    // Skip fixtures that don't produce sensor reports
    let skip_fixtures = ["broken-config", "tool-error"];

    // Auto-discover fixture directories
    for entry in discover_fixture_dirs(fixtures)? {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name for fixture dir"))?;
        if skip_fixtures.contains(&name) {
            continue;
        }

        let golden_name = format!("{name}.report.json");
        let golden_path = golden.join(&golden_name);

        let current = run_builddiag_sensor(runner, &entry)?;
        let current_normalized = normalize_for_comparison(&current)?;

        if update {
            std::fs::create_dir_all(golden)?;
            std::fs::write(&golden_path, &current)?;
            println!("    Updated {golden_path}");
        } else if golden_path.exists() {
            let golden_content = std::fs::read_to_string(&golden_path)?;
            let golden_normalized = normalize_for_comparison(&golden_content)?;

            if current_normalized != golden_normalized {
                anyhow::bail!(
                    "Output differs from golden file {golden_path}\n\nExpected:\n{}\n\nGot:\n{}",
                    golden_normalized,
                    current_normalized
                );
            }
            println!("    {golden_name}: OK");
        }
    }

    Ok(())
}

fn run_builddiag_sensor(runner: &dyn CommandRunner, root: &Utf8Path) -> Result<String> {
    let temp_dir = tempfile::TempDir::new()?;
    let out_file = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("report.json");

    let mut args = vec![
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
    ];

    // Auto-detect config file in the fixture directory
    let config_path = root.join(".builddiag.toml");
    let config_str;
    if config_path.exists() {
        config_str = config_path.to_string();
        args.extend(["--config", &config_str]);
    }

    let output = runner
        .output(&Cmd::new("cargo").args(args))
        .context("run builddiag")?;

    // builddiag might exit with 2 for policy violations, which is ok
    if !out_file.exists() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("builddiag did not produce output: {stderr}");
    }

    std::fs::read_to_string(&out_file).context("read builddiag output")
}

/// Discover all fixture directories under the given path.
///
/// Returns sorted list of directories that contain a `Cargo.toml`.
fn discover_fixture_dirs(fixtures: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(fixtures).with_context(|| format!("read {fixtures}"))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let utf8 = Utf8PathBuf::from_path_buf(path)
                .map_err(|_| anyhow::anyhow!("non-utf8 path in fixtures"))?;
            if utf8.join("Cargo.toml").exists() {
                dirs.push(utf8);
            }
        }
    }
    dirs.sort();
    Ok(dirs)
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

        // Also normalize git info (commit and branch might change)
        if let Some(git) = run.get_mut("git").and_then(|g| g.as_object_mut()) {
            git.insert(
                "commit".to_string(),
                serde_json::Value::String("NORMALIZED".to_string()),
            );
            git.insert(
                "branch".to_string(),
                serde_json::Value::String("NORMALIZED".to_string()),
            );
            git.insert("dirty".to_string(), serde_json::Value::Bool(false));
        }

        // Normalize host info (os/arch differ between dev and CI machines).
        if let Some(host) = run.get_mut("host").and_then(|h| h.as_object_mut()) {
            host.insert(
                "os".to_string(),
                serde_json::Value::String("NORMALIZED".to_string()),
            );
            host.insert(
                "arch".to_string(),
                serde_json::Value::String("NORMALIZED".to_string()),
            );
        }
    }

    serde_json::to_string_pretty(&value).context("serialize normalized JSON")
}

fn run_tool_error_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let tool_error_dir = fixtures.join("tool-error");
    if !tool_error_dir.exists() {
        return Ok(());
    }

    let temp_dir = tempfile::TempDir::new()?;
    let out_file = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("error-receipt.json");

    // Run in cockpit mode — should produce error receipt and exit 0
    let output = runner
        .output(&Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--root",
            tool_error_dir.as_str(),
            "--format",
            "sensor",
            "--mode",
            "cockpit",
            "--profile",
            "oss",
            "--out",
            out_file.as_str(),
        ]))
        .context("run builddiag on tool-error fixture")?;

    // Should exit 0 in cockpit mode (report written)
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Expected exit 0 in cockpit mode, got {}: {stderr}",
            output.status.code().unwrap_or(-1)
        );
    }

    if !out_file.exists() {
        anyhow::bail!("Error receipt was not written to {out_file}");
    }

    let receipt_content = std::fs::read_to_string(&out_file)?;
    let receipt: serde_json::Value =
        serde_json::from_str(&receipt_content).context("error receipt is not valid JSON")?;

    // Verify verdict is "error"
    let verdict = receipt
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if verdict != "error" {
        anyhow::bail!("Expected verdict 'error', got '{verdict}'");
    }
    println!("    verdict: error (OK)");

    // Verify finding has check_id = "tool.runtime" and code = "runtime_error"
    let findings = receipt
        .get("findings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing findings array"))?;

    if findings.is_empty() {
        anyhow::bail!("Expected at least one finding in error receipt");
    }

    let finding = &findings[0];
    let check_id = finding
        .get("check_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let code = finding.get("code").and_then(|v| v.as_str()).unwrap_or("");
    let severity = finding
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if check_id != "tool.runtime" {
        anyhow::bail!("Expected check_id 'tool.runtime', got '{check_id}'");
    }
    if code != "runtime_error" {
        anyhow::bail!("Expected code 'runtime_error', got '{code}'");
    }
    if severity != "error" {
        anyhow::bail!("Expected severity 'error', got '{severity}'");
    }

    println!("    finding: check_id=tool.runtime, code=runtime_error, severity=error (OK)");
    Ok(())
}

fn run_library_parity_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let valid_dir = fixtures.join("valid-workspace");
    if !valid_dir.exists() {
        return Ok(());
    }

    // Run via CLI (subprocess) to get sensor output
    let cli_output = run_builddiag_sensor(runner, &valid_dir)?;
    let cli_normalized = normalize_for_comparison(&cli_output)?;

    // Run via library (in-process) to get sensor output
    let settings = builddiag_core::Settings {
        root: valid_dir.clone(),
        config: builddiag_types::Config {
            profile: builddiag_types::Profile::Oss,
            ..Default::default()
        },
        allow_all: false,
        changed_files: None,
        cache_config: None,
        substrate: None,
    };

    let result = builddiag_core::run(&settings).context("builddiag_core::run() failed")?;

    let lib_json = serde_json::to_string_pretty(&result.sensor_report)
        .context("serialize library sensor report")?;
    let lib_normalized = normalize_for_comparison(&lib_json)?;

    if cli_normalized != lib_normalized {
        anyhow::bail!(
            "Library and CLI produce different sensor reports!\n\nCLI:\n{}\n\nLibrary:\n{}",
            cli_normalized,
            lib_normalized
        );
    }

    println!("    valid-workspace: Library output matches CLI output");
    Ok(())
}

/// Run builddiag in native (builddiag.report.v1) format on a fixture directory.
fn run_builddiag_native(runner: &dyn CommandRunner, root: &Utf8Path) -> Result<String> {
    let temp_dir = tempfile::TempDir::new()?;
    let out_file = Utf8Path::from_path(temp_dir.path())
        .unwrap()
        .join("report.json");

    let mut args = vec![
        "run",
        "-p",
        "builddiag",
        "--",
        "check",
        "--root",
        root.as_str(),
        "--format",
        "builddiag",
        "--profile",
        "oss",
        "--out",
        out_file.as_str(),
    ];

    // Auto-detect config file in the fixture directory
    let config_path = root.join(".builddiag.toml");
    let config_str;
    if config_path.exists() {
        config_str = config_path.to_string();
        args.extend(["--config", &config_str]);
    }

    let output = runner
        .output(&Cmd::new("cargo").args(args))
        .context("run builddiag (native format)")?;

    if !out_file.exists() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("builddiag (native) did not produce output: {stderr}");
    }

    std::fs::read_to_string(&out_file).context("read builddiag native output")
}

fn run_native_schema_validation(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let schema_path = workspace_root().join("schemas/builddiag.report.v1.schema.json");
    let schema_content = std::fs::read_to_string(&schema_path)
        .with_context(|| format!("read native schema from {schema_path}"))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_content).context("parse native schema JSON")?;

    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| anyhow::anyhow!("compile native schema: {e}"))?;

    // Skip fixtures that don't produce normal reports
    let skip_fixtures = ["broken-config", "tool-error"];
    for entry in discover_fixture_dirs(fixtures)? {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name for fixture dir"))?;
        if skip_fixtures.contains(&name) {
            continue;
        }

        let report = run_builddiag_native(runner, &entry)?;
        let report_value: serde_json::Value =
            serde_json::from_str(&report).with_context(|| format!("parse {name} native report"))?;

        if let Err(e) = validator.validate(&report_value) {
            anyhow::bail!("{name} native report failed schema validation: {}", e);
        }
        println!("    {name}: OK");
    }

    Ok(())
}

fn run_verdict_contract_check(runner: &dyn CommandRunner, fixtures: &Utf8Path) -> Result<()> {
    let fingerprint_re = regex::Regex::new(r"^[0-9a-f]{64}$").unwrap();
    let valid_reasons: &[&str] = &[
        builddiag_types::verdict_reasons::CHECKS_FAILED,
        builddiag_types::verdict_reasons::CHECKS_WARNED,
        builddiag_types::verdict_reasons::ALL_CHECKS_SKIPPED,
        builddiag_types::verdict_reasons::TOOL_ERROR,
    ];

    let skip_fixtures = ["broken-config", "tool-error"];
    for entry in discover_fixture_dirs(fixtures)? {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name for fixture dir"))?;
        if skip_fixtures.contains(&name) {
            continue;
        }

        let report_str = run_builddiag_sensor(runner, &entry)?;
        let report: serde_json::Value = serde_json::from_str(&report_str)
            .with_context(|| format!("parse {name} sensor report"))?;

        // Validate fingerprints on all findings
        if let Some(findings) = report.get("findings").and_then(|v| v.as_array()) {
            for (i, finding) in findings.iter().enumerate() {
                if let Some(fp) = finding.get("fingerprint").and_then(|v| v.as_str()) {
                    if !fingerprint_re.is_match(fp) {
                        anyhow::bail!(
                            "{name}: finding[{i}] fingerprint is not 64-char hex: '{fp}'"
                        );
                    }
                } else {
                    anyhow::bail!("{name}: finding[{i}] missing fingerprint");
                }
            }
        }

        // Validate verdict contract
        let verdict = report
            .get("verdict")
            .ok_or_else(|| anyhow::anyhow!("{name}: missing verdict"))?;
        let status = verdict
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("{name}: missing verdict.status"))?;
        let reasons: Vec<&str> = verdict
            .get("reasons")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>())
            .unwrap_or_default();
        let data = verdict.get("data");

        match status {
            "pass" => {
                if !reasons.is_empty() {
                    anyhow::bail!(
                        "{name}: pass verdict should have empty reasons, got {reasons:?}"
                    );
                }
                if data.is_some() && !data.unwrap().is_null() {
                    anyhow::bail!("{name}: pass verdict should have no data");
                }
            }
            "skip" => {
                if !reasons.contains(&builddiag_types::verdict_reasons::ALL_CHECKS_SKIPPED) {
                    anyhow::bail!(
                        "{name}: skip verdict should have 'all_checks_skipped' reason, got {reasons:?}"
                    );
                }
            }
            "warn" | "fail" => {
                if reasons.is_empty() {
                    anyhow::bail!("{name}: {status} verdict should have at least one reason token");
                }
                for r in &reasons {
                    if !valid_reasons.contains(r) {
                        anyhow::bail!("{name}: unknown reason token '{r}'");
                    }
                }
                // Warn/fail verdicts with check-level failures should have data
                if (reasons.contains(&builddiag_types::verdict_reasons::CHECKS_FAILED)
                    || reasons.contains(&builddiag_types::verdict_reasons::CHECKS_WARNED))
                    && (data.is_none() || data.unwrap().is_null())
                {
                    anyhow::bail!(
                        "{name}: {status} verdict with checks_failed/checks_warned should have data"
                    );
                }
            }
            _ => {
                anyhow::bail!("{name}: unexpected verdict status '{status}'");
            }
        }

        println!("    {name}: OK (status={status}, reasons={reasons:?})");
    }

    Ok(())
}

fn run_native_golden_check(
    runner: &dyn CommandRunner,
    fixtures: &Utf8Path,
    golden: &Utf8Path,
    update: bool,
) -> Result<()> {
    let skip_fixtures = ["broken-config", "tool-error"];

    for entry in discover_fixture_dirs(fixtures)? {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name for fixture dir"))?;
        if skip_fixtures.contains(&name) {
            continue;
        }

        let golden_name = format!("{name}.native.report.json");
        let golden_path = golden.join(&golden_name);

        let current = run_builddiag_native(runner, &entry)?;
        let current_normalized = normalize_for_comparison(&current)?;

        if update {
            std::fs::create_dir_all(golden)?;
            std::fs::write(&golden_path, &current)?;
            println!("    Updated {golden_path}");
        } else if golden_path.exists() {
            let golden_content = std::fs::read_to_string(&golden_path)?;
            let golden_normalized = normalize_for_comparison(&golden_content)?;

            if current_normalized != golden_normalized {
                anyhow::bail!(
                    "Native output differs from golden file {golden_path}\n\nExpected:\n{}\n\nGot:\n{}",
                    golden_normalized,
                    current_normalized
                );
            }
            println!("    {golden_name}: OK");
        }
    }

    Ok(())
}

fn run_ci(runner: &dyn CommandRunner) -> Result<()> {
    // Use a separate target directory to avoid locking issues on Windows.
    // When xtask runs `cargo run -p xtask`, it would try to overwrite the
    // running xtask.exe, which fails on Windows. Using target/ci sidesteps this.
    let target_dir = Some("target/ci");

    let fmt_args = ["cargo", "fmt", "--all", "--", "--check"];
    let clippy_args = [
        "cargo",
        "clippy",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ];
    let schema_args = ["cargo", "run", "-p", "xtask", "--", "schema"];
    let conform_args = ["cargo", "run", "-p", "xtask", "--", "conform"];

    run_with_target_dir(runner, &fmt_args, target_dir)?;
    run_with_target_dir(runner, &clippy_args, target_dir)?;
    run_with_target_dir(runner, &["cargo", "test", "--all"], target_dir)?;
    run_with_target_dir(runner, &schema_args, target_dir)?;
    run_with_target_dir(runner, &conform_args, target_dir)?;
    Ok(())
}

fn run_with_target_dir(
    runner: &dyn CommandRunner,
    args: &[&str],
    target_dir: Option<&str>,
) -> Result<()> {
    let cmd_name = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("no command provided"))?;
    let mut cmd = Cmd::new(*cmd_name).args(args.iter().skip(1).copied());
    if let Some(dir) = target_dir {
        cmd = cmd.env("CARGO_TARGET_DIR", dir);
    }
    let status = runner
        .status(&cmd)
        .with_context(|| format!("run {cmd_name}"))?;
    if !status.success() {
        anyhow::bail!("command failed: {cmd_name}");
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
fn run_coverage(
    runner: &dyn CommandRunner,
    out_dir: &Utf8Path,
    html: bool,
    open: bool,
) -> Result<()> {
    // Check if cargo-llvm-cov is installed
    let check = runner.output(&Cmd::new("cargo").args(["llvm-cov", "--version"]));

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

    let status = runner
        .status(&Cmd::new("cargo").args([
            "llvm-cov",
            "--all-features",
            "--workspace",
            "--lcov",
            "--output-path",
            lcov_path.as_str(),
        ]))
        .context("run cargo llvm-cov")?;

    if !status.success() {
        anyhow::bail!("cargo llvm-cov failed");
    }

    println!("LCOV report generated: {lcov_path}");

    // Generate HTML report if requested
    if html {
        let html_dir = out_dir.join("html");
        println!("Generating HTML coverage report: {html_dir}");

        let status = runner
            .status(&Cmd::new("cargo").args([
                "llvm-cov",
                "--all-features",
                "--workspace",
                "--html",
                "--output-dir",
                html_dir.as_str(),
            ]))
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
            let _ = runner.status(&Cmd::new("open").args([index_path.as_str()]));
            #[cfg(target_os = "linux")]
            let _ = runner.status(&Cmd::new("xdg-open").args([index_path.as_str()]));
            #[cfg(target_os = "windows")]
            let _ = runner.status(&Cmd::new("cmd").args(["/C", "start", "", index_path.as_str()]));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn status_from_code(code: i32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code as u32)
        }
    }

    fn success_status() -> std::process::ExitStatus {
        status_from_code(0)
    }

    fn output_with_status(code: i32) -> std::process::Output {
        std::process::Output {
            status: status_from_code(code),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn success_output() -> std::process::Output {
        output_with_status(0)
    }

    type OutputFn = Box<dyn Fn(&Cmd) -> Result<std::process::Output> + Send + Sync>;
    type StatusFn = Box<dyn Fn(&Cmd) -> Result<std::process::ExitStatus> + Send + Sync>;

    struct FakeRunner {
        commands: Mutex<Vec<Cmd>>,
        output_fn: OutputFn,
        status_fn: StatusFn,
    }

    impl FakeRunner {
        fn new() -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                output_fn: Box::new(|_| Ok(success_output())),
                status_fn: Box::new(|_| Ok(success_status())),
            }
        }

        fn with_output_fn<F>(mut self, f: F) -> Self
        where
            F: Fn(&Cmd) -> Result<std::process::Output> + Send + Sync + 'static,
        {
            self.output_fn = Box::new(f);
            self
        }

        fn with_status_fn<F>(mut self, f: F) -> Self
        where
            F: Fn(&Cmd) -> Result<std::process::ExitStatus> + Send + Sync + 'static,
        {
            self.status_fn = Box::new(f);
            self
        }

        fn recorded(&self) -> Vec<Cmd> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl Default for FakeRunner {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CommandRunner for FakeRunner {
        fn status(&self, cmd: &Cmd) -> Result<std::process::ExitStatus> {
            self.commands.lock().unwrap().push(cmd.clone());
            (self.status_fn)(cmd)
        }

        fn output(&self, cmd: &Cmd) -> Result<std::process::Output> {
            self.commands.lock().unwrap().push(cmd.clone());
            (self.output_fn)(cmd)
        }
    }

    fn fixtures_root() -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .join("..")
            .join("fixtures")
            .join("conformance")
    }

    fn golden_root() -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .join("..")
            .join("fixtures")
            .join("golden")
    }

    fn read_golden(golden: &Utf8Path, name: &str, kind: &str) -> String {
        let filename = match kind {
            "sensor" => format!("{name}.report.json"),
            "native" => format!("{name}.native.report.json"),
            _ => panic!("unknown golden kind"),
        };
        std::fs::read_to_string(golden.join(filename)).unwrap()
    }

    fn write_text(path: &Utf8Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn arg_value(args: &[String], key: &str) -> Option<String> {
        args.iter()
            .position(|a| a == key)
            .and_then(|idx| args.get(idx + 1))
            .cloned()
    }

    fn fixture_root_with(name: &str) -> (TempDir, Utf8PathBuf, Utf8PathBuf) {
        let temp = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let fixture = root.join(name);
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::write(
            fixture.join("Cargo.toml"),
            r#"[package]
name = "fixture"
version = "0.1.0"
"#,
        )
        .unwrap();
        (temp, root, fixture)
    }

    fn runner_writing_out_json(json: serde_json::Value, status_code: i32) -> FakeRunner {
        let body = serde_json::to_string_pretty(&json).unwrap();
        FakeRunner::new().with_output_fn(move |cmd| {
            if let Some(out) = arg_value(&cmd.args, "--out") {
                let out_path = Utf8PathBuf::from(out);
                write_text(&out_path, &body)?;
            }
            Ok(output_with_status(status_code))
        })
    }

    fn layout_runner(
        report: Option<serde_json::Value>,
        write_comment: bool,
        payload: Option<serde_json::Value>,
        status_code: i32,
    ) -> FakeRunner {
        FakeRunner::new().with_output_fn(move |cmd| {
            if let Some(dir) = arg_value(&cmd.args, "--artifacts-dir") {
                let artifacts_dir = Utf8PathBuf::from(dir);
                if let Some(report) = report.as_ref() {
                    write_text(
                        &artifacts_dir.join("report.json"),
                        &serde_json::to_string_pretty(report).unwrap(),
                    )?;
                }
                if write_comment {
                    write_text(&artifacts_dir.join("comment.md"), "# ok\n")?;
                }
                if let Some(payload) = payload.as_ref() {
                    write_text(
                        &artifacts_dir.join("extras/payload.json"),
                        &serde_json::to_string_pretty(payload).unwrap(),
                    )?;
                }
            }
            Ok(output_with_status(status_code))
        })
    }

    fn valid_layout_report() -> serde_json::Value {
        serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": [
                { "location": { "path": "src/lib.rs", "line": 1, "col": 1 } }
            ]
        })
    }

    fn payload_report(schema: &str) -> serde_json::Value {
        serde_json::json!({ "schema": schema })
    }

    fn tool_error_receipt(
        verdict: &str,
        check_id: &str,
        code: &str,
        severity: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "schema": "builddiag.report.v1",
            "verdict": verdict,
            "findings": [
                {
                    "check_id": check_id,
                    "code": code,
                    "severity": severity
                }
            ]
        })
    }

    fn verdict_report(
        findings: serde_json::Value,
        verdict: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "schema": "sensor.report.v1",
            "findings": findings,
            "verdict": verdict
        })
    }

    fn finding_with_fingerprint(fp: Option<&str>) -> serde_json::Value {
        match fp {
            Some(fp) => serde_json::json!({ "fingerprint": fp }),
            None => serde_json::json!({}),
        }
    }

    fn stubbed_runner() -> FakeRunner {
        let golden = golden_root();
        FakeRunner::new().with_output_fn(move |cmd| {
            if cmd.program != "cargo" {
                return Ok(success_output());
            }
            if !cmd.args.iter().any(|a| a == "builddiag") {
                return Ok(success_output());
            }

            let root = arg_value(&cmd.args, "--root");
            let root_name = root
                .as_ref()
                .and_then(|r| Utf8Path::new(r).file_name())
                .unwrap_or("");
            let out_path = arg_value(&cmd.args, "--out");
            let artifacts_dir = arg_value(&cmd.args, "--artifacts-dir");
            let format = arg_value(&cmd.args, "--format").unwrap_or_default();
            let mode = arg_value(&cmd.args, "--mode").unwrap_or_default();

            if let Some(dir) = artifacts_dir {
                let artifacts_dir = Utf8PathBuf::from(dir);
                let report_path = artifacts_dir.join("report.json");
                let comment_path = artifacts_dir.join("comment.md");
                let payload_path = artifacts_dir.join("extras/payload.json");
                let report = serde_json::json!({
                    "schema": "sensor.report.v1",
                    "artifacts": [
                        {"name": "payload", "path": "extras/payload.json"},
                        {"name": "comment", "path": "comment.md"}
                    ],
                    "findings": []
                });
                write_text(
                    &report_path,
                    &serde_json::to_string_pretty(&report).unwrap(),
                )?;
                write_text(&comment_path, "# ok\n")?;
                write_text(&payload_path, r#"{ "schema": "builddiag.report.v1" }"#)?;
                return Ok(success_output());
            }

            if let Some(out) = out_path {
                let out_path = Utf8PathBuf::from(out);
                if mode == "cockpit" && root_name == "tool-error" {
                    let receipt = serde_json::json!({
                        "schema": "builddiag.report.v1",
                        "verdict": "error",
                        "findings": [
                            {
                                "check_id": "tool.runtime",
                                "code": "runtime_error",
                                "severity": "error"
                            }
                        ]
                    });
                    write_text(&out_path, &serde_json::to_string_pretty(&receipt).unwrap())?;
                } else if mode == "cockpit" && root_name == "broken-config" {
                    write_text(&out_path, r#"{ "error": true }"#)?;
                } else if format == "builddiag" {
                    let native = read_golden(&golden, root_name, "native");
                    write_text(&out_path, &native)?;
                } else {
                    let sensor = read_golden(&golden, root_name, "sensor");
                    write_text(&out_path, &sensor)?;
                }
            }

            Ok(success_output())
        })
    }

    #[test]
    #[should_panic(expected = "unknown golden kind")]
    fn read_golden_panics_on_unknown_kind() {
        let golden = golden_root();
        let _ = read_golden(&golden, "case", "unknown");
    }

    #[test]
    fn write_text_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let path = root.join("nested/file.txt");
        write_text(&path, "ok").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn write_text_handles_no_parent() {
        let filename = format!("xtask-temp-{}.txt", std::process::id());
        let path = Utf8PathBuf::from(filename.as_str());
        write_text(&path, "ok").unwrap();
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn write_text_errors_when_parent_is_file() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let parent = root.join("blocked");
        std::fs::write(&parent, "not a dir").unwrap();
        let path = parent.join("file.txt");
        assert!(write_text(&path, "ok").is_err());
    }

    #[test]
    fn write_text_errors_on_empty_path() {
        let path = Utf8PathBuf::from("");
        assert!(write_text(&path, "ok").is_err());
    }

    #[test]
    fn runner_writing_out_json_writes_to_out_path() {
        let temp = TempDir::new().unwrap();
        let out_path = Utf8PathBuf::from_path_buf(temp.path().join("out.json")).unwrap();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--out",
            out_path.as_str(),
        ]);
        runner.output(&cmd).unwrap();
        assert!(out_path.exists());
    }

    #[test]
    fn runner_writing_out_json_skips_without_out_arg() {
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);
        let cmd = Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]);
        let output = runner.output(&cmd).unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn layout_runner_writes_artifacts() {
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        runner.output(&cmd).unwrap();
        assert!(artifacts_dir.join("report.json").exists());
        assert!(artifacts_dir.join("comment.md").exists());
        assert!(artifacts_dir.join("extras/payload.json").exists());
    }

    #[test]
    fn layout_runner_skips_optional_outputs() {
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        let runner = layout_runner(None, false, None, 0);
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        runner.output(&cmd).unwrap();
        assert!(!artifacts_dir.join("report.json").exists());
        assert!(!artifacts_dir.join("comment.md").exists());
        assert!(!artifacts_dir.join("extras/payload.json").exists());
    }

    #[test]
    fn layout_runner_skips_without_artifacts_dir_arg() {
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        let cmd = Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]);
        let output = runner.output(&cmd).unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn layout_runner_errors_when_report_write_fails() {
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        std::fs::create_dir_all(artifacts_dir.parent().unwrap()).unwrap();
        std::fs::write(&artifacts_dir, "not a dir").unwrap();
        let runner = layout_runner(Some(valid_layout_report()), false, None, 0);
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        assert!(runner.output(&cmd).is_err());
    }

    #[test]
    fn layout_runner_errors_when_payload_write_fails() {
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        std::fs::create_dir_all(&artifacts_dir).unwrap();
        std::fs::write(artifacts_dir.join("extras"), "not a dir").unwrap();
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        assert!(runner.output(&cmd).is_err());
    }

    #[test]
    fn stubbed_runner_handles_non_cargo() {
        let runner = stubbed_runner();
        let output = runner.output(&Cmd::new("echo").args(["hi"])).unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn stubbed_runner_handles_non_builddiag_cargo() {
        let runner = stubbed_runner();
        let output = runner
            .output(&Cmd::new("cargo").args(["fmt", "--all"]))
            .unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn stubbed_runner_handles_builddiag_without_outputs() {
        let runner = stubbed_runner();
        let output = runner
            .output(&Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]))
            .unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn stubbed_runner_writes_artifacts_dir() {
        let runner = stubbed_runner();
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        runner.output(&cmd).unwrap();
        assert!(artifacts_dir.join("report.json").exists());
        assert!(artifacts_dir.join("comment.md").exists());
        assert!(artifacts_dir.join("extras/payload.json").exists());
    }

    #[test]
    fn stubbed_runner_errors_when_artifacts_dir_is_file() {
        let runner = stubbed_runner();
        let temp = TempDir::new().unwrap();
        let artifacts_dir =
            Utf8PathBuf::from_path_buf(temp.path().join("artifacts/builddiag")).unwrap();
        std::fs::create_dir_all(artifacts_dir.parent().unwrap()).unwrap();
        std::fs::write(&artifacts_dir, "not a dir").unwrap();
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--artifacts-dir",
            artifacts_dir.as_str(),
        ]);
        assert!(runner.output(&cmd).is_err());
    }

    #[test]
    fn stubbed_runner_writes_out_path() {
        let runner = stubbed_runner();
        let temp = TempDir::new().unwrap();
        let out_path = Utf8PathBuf::from_path_buf(temp.path().join("out.json")).unwrap();
        let root = fixtures_root().join("valid-workspace");
        let cmd = Cmd::new("cargo").args([
            "run",
            "-p",
            "builddiag",
            "--",
            "check",
            "--root",
            root.as_str(),
            "--out",
            out_path.as_str(),
        ]);
        runner.output(&cmd).unwrap();
        assert!(out_path.exists());
    }

    #[test]
    fn normalize_for_comparison_strips_timestamps() {
        let input = r#"
        {
          "run": {
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:00:01Z",
            "duration_ms": 10,
            "git": { "commit": "abc", "dirty": true }
          }
        }"#;

        let normalized = normalize_for_comparison(input).unwrap();
        let value: serde_json::Value = serde_json::from_str(&normalized).unwrap();
        let run = value.get("run").and_then(|v| v.as_object()).unwrap();
        assert!(!run.contains_key("started_at"));
        assert!(!run.contains_key("ended_at"));
        assert!(!run.contains_key("duration_ms"));
        let git = run.get("git").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            git.get("commit").and_then(|v| v.as_str()),
            Some("NORMALIZED")
        );
        assert_eq!(git.get("dirty").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn discover_fixture_dirs_finds_cargo_toml() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(
            root.join("a/Cargo.toml"),
            "[package]\nname='a'\nversion='0.1.0'\n",
        )
        .unwrap();

        let dirs = discover_fixture_dirs(&root).unwrap();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("a"));
    }

    #[test]
    fn discover_fixture_dirs_skips_non_dirs() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        write_text(&root.join("note.txt"), "hi").unwrap();
        let dirs = discover_fixture_dirs(&root).unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn write_json_creates_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let path = root.join("out.json");

        write_json(path.clone(), &serde_json::json!({"ok": true})).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"ok\""));
    }

    #[test]
    fn generate_schemas_creates_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        generate_schemas(&root).unwrap();
        assert!(root.join("builddiag.report.v1.schema.json").exists());
        assert!(root.join("builddiag.config.v1.schema.json").exists());
    }

    #[test]
    fn run_with_target_dir_sets_env() {
        let runner = FakeRunner::default();
        run_with_target_dir(&runner, &["cargo", "test", "--all"], Some("target/ci")).unwrap();

        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 1);
        let env = &recorded[0].env;
        assert!(
            env.iter()
                .any(|(k, v)| k == "CARGO_TARGET_DIR" && v == "target/ci")
        );
    }

    #[test]
    fn run_ci_invokes_expected_commands() {
        let runner = FakeRunner::default();
        run_ci(&runner).unwrap();
        let recorded = runner.recorded();
        assert!(recorded.len() >= 5);
        assert!(recorded.iter().any(|c| c.args.contains(&"fmt".to_string())));
        assert!(
            recorded
                .iter()
                .any(|c| c.args.contains(&"clippy".to_string()))
        );
    }

    #[test]
    fn run_coverage_executes_commands() {
        let runner = FakeRunner::default();
        let temp = tempfile::TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

        run_coverage(&runner, &out_dir, true, true).unwrap();
        assert!(out_dir.exists());

        let recorded = runner.recorded();
        assert!(recorded.iter().any(|c| c.program == "cargo"));
    }

    #[test]
    fn run_conform_all_checks_passes_with_stubbed_runner() {
        let runner = stubbed_runner();
        let fixtures = fixtures_root();
        let golden = golden_root();
        run_conform(&runner, &fixtures, &golden, false, None).unwrap();
    }

    #[test]
    fn run_conform_reports_failure_for_invalid_schema() {
        let fixtures = fixtures_root();
        let golden = golden_root();
        let runner = FakeRunner::new().with_output_fn(move |cmd| {
            if cmd.program == "cargo"
                && cmd.args.iter().any(|a| a == "builddiag")
                && let Some(out) = arg_value(&cmd.args, "--out")
            {
                let out_path = Utf8PathBuf::from(out);
                write_text(&out_path, "{")?;
            }
            Ok(success_output())
        });
        runner
            .output(&Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]))
            .unwrap();
        let noop = runner.output(&Cmd::new("echo").args(["noop"])).unwrap();
        assert!(noop.status.success());

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["schema".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_determinism() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let golden = golden_root();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let runner = {
            let counter = counter.clone();
            FakeRunner::new().with_output_fn(move |cmd| {
                if let Some(out) = arg_value(&cmd.args, "--out") {
                    let out_path = Utf8PathBuf::from(out);
                    let idx = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let verdict = if idx == 0 { "pass" } else { "fail" };
                    let report = serde_json::json!({
                        "schema": "sensor.report.v1",
                        "verdict": verdict
                    });
                    write_text(&out_path, &serde_json::to_string_pretty(&report).unwrap())?;
                }
                Ok(success_output())
            })
        };
        runner
            .output(&Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]))
            .unwrap();

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["determinism".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_survivability() {
        let (_temp, fixtures, fixture) = fixture_root_with("broken-config");
        write_text(&fixture.join(".builddiag.toml"), "bad = true").unwrap();
        let golden = golden_root();
        let runner = FakeRunner::default();

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["survivability".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_layout() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let golden = golden_root();
        let runner = layout_runner(None, true, Some(payload_report("builddiag.report.v1")), 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["layout".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_updates_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);

        run_conform(
            &runner,
            &fixtures,
            &golden,
            true,
            Some(vec!["golden".to_string()]),
        )
        .unwrap();
        assert!(golden.join("case.report.json").exists());
    }

    #[test]
    fn run_conform_reports_failure_for_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.report.json"),
            r#"{ "schema": "sensor.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["golden".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_tool_error() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let golden = golden_root();
        let runner = FakeRunner::default();

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["tool-error".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_library_parity() {
        let fixtures = fixtures_root();
        let golden = golden_root();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["library-parity".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_native_schema() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let golden = golden_root();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "wrong.schema"}), 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["native-schema".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_reports_failure_for_verdict_contract() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let golden = golden_root();
        let report = verdict_report(
            serde_json::json!([finding_with_fingerprint(Some("bad"))]),
            serde_json::json!({"status": "pass", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["verdict-contract".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_conform_updates_native_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner =
            runner_writing_out_json(serde_json::json!({"schema": "builddiag.report.v1"}), 0);

        run_conform(
            &runner,
            &fixtures,
            &golden,
            true,
            Some(vec!["native-golden".to_string()]),
        )
        .unwrap();
        assert!(golden.join("case.native.report.json").exists());
    }

    #[test]
    fn run_conform_reports_failure_for_native_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.native.report.json"),
            r#"{ "schema": "builddiag.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();
        let runner =
            runner_writing_out_json(serde_json::json!({"schema": "builddiag.report.v1"}), 0);

        let result = run_conform(
            &runner,
            &fixtures,
            &golden,
            false,
            Some(vec!["native-golden".to_string()]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn cmd_builds_args_and_env() {
        let cmd = Cmd::new("cargo").args(["fmt", "--all"]).env("FOO", "bar");
        assert_eq!(cmd.program, "cargo");
        assert_eq!(cmd.args, vec!["fmt".to_string(), "--all".to_string()]);
        assert_eq!(cmd.env, vec![("FOO".to_string(), "bar".to_string())]);
    }

    #[test]
    #[cfg(windows)]
    fn real_command_runner_status_success() {
        let runner = RealCommandRunner;
        let status = runner
            .status(&Cmd::new("cmd").args(["/C", "exit", "0"]).env("FOO", "bar"))
            .unwrap();
        assert!(status.success());
    }

    #[test]
    #[cfg(unix)]
    fn real_command_runner_status_success() {
        let runner = RealCommandRunner;
        let status = runner
            .status(&Cmd::new("sh").args(["-c", "exit 0"]).env("FOO", "bar"))
            .unwrap();
        assert!(status.success());
    }

    #[test]
    #[cfg(windows)]
    fn real_command_runner_output_success() {
        let runner = RealCommandRunner;
        let output = runner
            .output(
                &Cmd::new("cmd")
                    .args(["/C", "echo", "hello"])
                    .env("FOO", "bar"),
            )
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello"));
    }

    #[test]
    #[cfg(unix)]
    fn real_command_runner_output_success() {
        let runner = RealCommandRunner;
        let output = runner
            .output(
                &Cmd::new("sh")
                    .args(["-c", "printf hello"])
                    .env("FOO", "bar"),
            )
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello"));
    }

    #[test]
    fn run_cli_dispatches_schema() {
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_cli(
            Cli {
                cmd: Command::Schema {
                    out_dir: out_dir.clone(),
                },
            },
            &runner,
        )
        .unwrap();

        assert!(out_dir.join("builddiag.report.v1.schema.json").exists());
    }

    #[test]
    fn run_cli_dispatches_ci() {
        let runner = FakeRunner::default();
        run_cli(Cli { cmd: Command::Ci }, &runner).unwrap();
        let recorded = runner.recorded();
        assert!(recorded.iter().any(|c| c.args.contains(&"fmt".to_string())));
    }

    #[test]
    fn run_cli_dispatches_coverage() {
        let runner = FakeRunner::default();
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        run_cli(
            Cli {
                cmd: Command::Coverage {
                    out_dir: out_dir.clone(),
                    html: false,
                    open: false,
                },
            },
            &runner,
        )
        .unwrap();
        assert!(out_dir.exists());
    }

    #[test]
    fn run_cli_dispatches_conform() {
        let runner = stubbed_runner();
        let fixtures = fixtures_root();
        let golden = golden_root();
        run_cli(
            Cli {
                cmd: Command::Conform {
                    fixtures,
                    golden,
                    update_golden: false,
                    only: None,
                },
            },
            &runner,
        )
        .unwrap();
    }

    #[test]
    fn run_main_returns_zero_on_success() {
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();
        let code = run_main(
            Cli {
                cmd: Command::Schema { out_dir },
            },
            &runner,
        );
        assert_eq!(code, 0);
    }

    #[test]
    fn run_main_returns_one_on_error() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().join("missing-fixtures")).unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp.path().join("golden")).unwrap();
        let runner = FakeRunner::default();
        let code = run_main(
            Cli {
                cmd: Command::Conform {
                    fixtures,
                    golden,
                    update_golden: false,
                    only: None,
                },
            },
            &runner,
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn try_main_from_parses_schema() {
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();
        let args = [
            std::ffi::OsString::from("xtask"),
            std::ffi::OsString::from("schema"),
            std::ffi::OsString::from("--out-dir"),
            std::ffi::OsString::from(out_dir.as_str()),
        ];
        try_main_from(args, &runner).unwrap();
        assert!(out_dir.join("builddiag.config.v1.schema.json").exists());
    }

    #[test]
    fn main_uses_injected_args() {
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let args = vec![
            std::ffi::OsString::from("xtask"),
            std::ffi::OsString::from("schema"),
            std::ffi::OsString::from("--out-dir"),
            std::ffi::OsString::from(out_dir.as_str()),
        ];
        *super::MAIN_ARGS.lock().unwrap() = Some(args);
        let code = main();
        assert_eq!(code, std::process::ExitCode::from(0));
        assert!(out_dir.join("builddiag.report.v1.schema.json").exists());
    }

    #[test]
    fn main_args_falls_back_to_env() {
        *super::MAIN_ARGS.lock().unwrap() = None;
        let args = main_args();
        assert!(!args.is_empty());
    }

    #[test]
    fn run_schema_validation_fails_on_invalid_schema() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let runner = runner_writing_out_json(serde_json::json!({}), 0);

        let result = run_schema_validation(&runner, &fixtures);
        assert!(result.is_err());
    }

    #[test]
    fn run_determinism_check_skips_when_fixture_missing() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_determinism_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_determinism_check_detects_non_determinism() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let counter = std::sync::atomic::AtomicUsize::new(0);
        let runner = FakeRunner::new().with_output_fn(move |cmd| {
            if let Some(out) = arg_value(&cmd.args, "--out") {
                let idx = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let verdict = if idx == 0 { "pass" } else { "fail" };
                let report = serde_json::json!({
                    "schema": "sensor.report.v1",
                    "verdict": verdict
                });
                let out_path = Utf8PathBuf::from(out);
                write_text(&out_path, &serde_json::to_string_pretty(&report).unwrap())?;
            }
            Ok(success_output())
        });
        runner
            .output(&Cmd::new("cargo").args(["run", "-p", "builddiag", "--", "check"]))
            .unwrap();

        let result = run_determinism_check(&runner, &fixtures);
        assert!(result.is_err());
    }

    #[test]
    fn run_survivability_check_skips_when_fixture_missing() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_survivability_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_survivability_check_errors_when_receipt_missing() {
        let (_temp, fixtures, fixture) = fixture_root_with("broken-config");
        write_text(&fixture.join(".builddiag.toml"), "bad = true").unwrap();
        let runner = FakeRunner::default();

        let result = run_survivability_check(&runner, &fixtures);
        assert!(result.is_err());
    }

    #[test]
    fn run_layout_check_skips_when_fixture_missing() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_layout_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_tool_error_check_skips_when_fixture_missing() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_tool_error_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_library_parity_check_skips_when_fixture_missing() {
        let temp = TempDir::new().unwrap();
        let fixtures = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let runner = FakeRunner::default();

        run_library_parity_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_layout_check_errors_when_report_missing() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(None, true, Some(payload_report("builddiag.report.v1")), 0);
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_on_schema_mismatch() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let mut report = valid_layout_report();
        report["schema"] = serde_json::Value::String("wrong.schema".to_string());
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_when_comment_missing() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(
            Some(valid_layout_report()),
            false,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_when_payload_missing() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(Some(valid_layout_report()), true, None, 0);
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_when_payload_schema_wrong() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("wrong.schema")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_passes_with_valid_report() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        run_layout_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_layout_check_allows_missing_findings() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ]
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        run_layout_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_layout_check_allows_missing_optional_paths() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload" },
                { "name": "comment" }
            ],
            "findings": [
                {}
            ]
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        run_layout_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_layout_check_errors_on_artifact_traversal() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "../payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": []
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_on_artifact_absolute_path() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "C:/abs/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": []
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_missing_payload_artifact_entry() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": []
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_missing_comment_artifact_entry() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" }
            ],
            "findings": []
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_missing_artifacts_array() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "findings": []
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_on_finding_backslash() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": [
                { "location": { "path": "src\\\\lib.rs" } }
            ]
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_on_finding_traversal() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": [
                { "location": { "path": "../src/lib.rs" } }
            ]
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_errors_on_finding_absolute_path() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "artifacts": [
                { "name": "payload", "path": "extras/payload.json" },
                { "name": "comment", "path": "comment.md" }
            ],
            "findings": [
                { "location": { "path": "/src/lib.rs" } }
            ]
        });
        let runner = layout_runner(
            Some(report),
            true,
            Some(payload_report("builddiag.report.v1")),
            0,
        );
        assert!(run_layout_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_layout_check_notes_nonzero_exit() {
        let (_temp, fixtures, _fixture) = fixture_root_with("valid-workspace");
        let runner = layout_runner(
            Some(valid_layout_report()),
            true,
            Some(payload_report("builddiag.report.v1")),
            1,
        );
        run_layout_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_golden_check_updates_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);

        run_golden_check(&runner, &fixtures, &golden, true).unwrap();
        assert!(golden.join("case.report.json").exists());
    }

    #[test]
    fn run_golden_check_matches_existing_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.report.json"),
            r#"{ "schema": "sensor.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();

        let runner = runner_writing_out_json(
            serde_json::json!({"schema": "sensor.report.v1", "verdict": "pass"}),
            0,
        );

        run_golden_check(&runner, &fixtures, &golden, false).unwrap();
    }

    #[test]
    fn run_golden_check_missing_golden_is_ok() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner = runner_writing_out_json(serde_json::json!({"schema": "sensor.report.v1"}), 0);

        run_golden_check(&runner, &fixtures, &golden, false).unwrap();
    }

    #[test]
    fn run_golden_check_detects_mismatch() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.report.json"),
            r#"{ "schema": "sensor.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();

        let runner = runner_writing_out_json(
            serde_json::json!({"schema": "sensor.report.v1", "verdict": "fail"}),
            0,
        );

        let result = run_golden_check(&runner, &fixtures, &golden, false);
        assert!(result.is_err());
    }

    #[test]
    fn run_builddiag_sensor_errors_when_output_missing() {
        let (_temp, _fixtures, fixture) = fixture_root_with("case");
        let runner = FakeRunner::default();
        let result = run_builddiag_sensor(&runner, &fixture);
        assert!(result.is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_nonzero_status() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            tool_error_receipt("error", "tool.runtime", "runtime_error", "error"),
            1,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_when_receipt_missing() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = FakeRunner::default();
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_wrong_verdict() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            tool_error_receipt("pass", "tool.runtime", "runtime_error", "error"),
            0,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_empty_findings() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            serde_json::json!({
                "schema": "builddiag.report.v1",
                "verdict": "error",
                "findings": []
            }),
            0,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_wrong_check_id() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            tool_error_receipt("error", "wrong.check", "runtime_error", "error"),
            0,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_wrong_code() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            tool_error_receipt("error", "tool.runtime", "wrong_code", "error"),
            0,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_tool_error_check_fails_on_wrong_severity() {
        let (_temp, fixtures, _fixture) = fixture_root_with("tool-error");
        let runner = runner_writing_out_json(
            tool_error_receipt("error", "tool.runtime", "runtime_error", "warn"),
            0,
        );
        assert!(run_tool_error_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_library_parity_check_detects_mismatch() {
        let fixtures = fixtures_root();
        let runner = runner_writing_out_json(
            serde_json::json!({"schema": "sensor.report.v1", "verdict": "error"}),
            0,
        );
        assert!(run_library_parity_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_builddiag_native_errors_when_output_missing() {
        let (_temp, _fixtures, fixture) = fixture_root_with("case");
        let runner = FakeRunner::default();
        let result = run_builddiag_native(&runner, &fixture);
        assert!(result.is_err());
    }

    #[test]
    fn run_native_schema_validation_fails_on_invalid_schema() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let runner = runner_writing_out_json(serde_json::json!({}), 0);
        let result = run_native_schema_validation(&runner, &fixtures);
        assert!(result.is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_invalid_fingerprint() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([finding_with_fingerprint(Some("abc"))]),
            serde_json::json!({"status": "pass", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_missing_fingerprint() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([finding_with_fingerprint(None)]),
            serde_json::json!({"status": "pass", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_pass_with_reasons() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "pass", "reasons": ["checks_failed"], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_pass_with_data() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "pass", "reasons": [], "data": {"extra": true}}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_skip_missing_reason() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "skip", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_warn_empty_reasons() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "warn", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_unknown_reason() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "fail", "reasons": ["bogus"], "data": {"info": 1}}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_fails_on_missing_data_for_checks_failed() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "fail", "reasons": ["checks_failed"], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_verdict_contract_check_accepts_skip_with_fingerprint() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let fp = "a".repeat(64);
        let report = verdict_report(
            serde_json::json!([finding_with_fingerprint(Some(&fp))]),
            serde_json::json!({
                "status": "skip",
                "reasons": [builddiag_types::verdict_reasons::ALL_CHECKS_SKIPPED],
                "data": null
            }),
        );
        let runner = runner_writing_out_json(report, 0);
        run_verdict_contract_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_verdict_contract_check_accepts_no_findings() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = serde_json::json!({
            "schema": "sensor.report.v1",
            "verdict": { "status": "pass", "reasons": [], "data": null }
        });
        let runner = runner_writing_out_json(report, 0);
        run_verdict_contract_check(&runner, &fixtures).unwrap();
    }

    #[test]
    fn run_verdict_contract_check_fails_on_unexpected_status() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let report = verdict_report(
            serde_json::json!([]),
            serde_json::json!({"status": "mystery", "reasons": [], "data": null}),
        );
        let runner = runner_writing_out_json(report, 0);
        assert!(run_verdict_contract_check(&runner, &fixtures).is_err());
    }

    #[test]
    fn run_native_golden_check_updates_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner =
            runner_writing_out_json(serde_json::json!({"schema": "builddiag.report.v1"}), 0);

        run_native_golden_check(&runner, &fixtures, &golden, true).unwrap();
        assert!(golden.join("case.native.report.json").exists());
    }

    #[test]
    fn run_native_golden_check_matches_existing_golden() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.native.report.json"),
            r#"{ "schema": "builddiag.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();
        let runner = runner_writing_out_json(
            serde_json::json!({"schema": "builddiag.report.v1", "verdict": "pass"}),
            0,
        );

        run_native_golden_check(&runner, &fixtures, &golden, false).unwrap();
    }

    #[test]
    fn run_native_golden_check_missing_golden_is_ok() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        let runner =
            runner_writing_out_json(serde_json::json!({"schema": "builddiag.report.v1"}), 0);

        run_native_golden_check(&runner, &fixtures, &golden, false).unwrap();
    }

    #[test]
    fn run_native_golden_check_detects_mismatch() {
        let (_temp, fixtures, _fixture) = fixture_root_with("case");
        let temp_golden = TempDir::new().unwrap();
        let golden = Utf8PathBuf::from_path_buf(temp_golden.path().to_path_buf()).unwrap();
        write_text(
            &golden.join("case.native.report.json"),
            r#"{ "schema": "builddiag.report.v1", "verdict": "pass" }"#,
        )
        .unwrap();

        let runner = runner_writing_out_json(
            serde_json::json!({"schema": "builddiag.report.v1", "verdict": "fail"}),
            0,
        );

        let result = run_native_golden_check(&runner, &fixtures, &golden, false);
        assert!(result.is_err());
    }

    #[test]
    fn run_with_target_dir_fails_on_error() {
        let runner = FakeRunner::new().with_status_fn(|_| Ok(status_from_code(1)));
        let result = run_with_target_dir(&runner, &["cargo", "test"], None);
        assert!(result.is_err());
    }

    #[test]
    fn run_coverage_errors_when_missing_llvm_cov() {
        let runner = FakeRunner::new().with_output_fn(|_| Ok(output_with_status(1)));
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let result = run_coverage(&runner, &out_dir, false, false);
        assert!(result.is_err());
    }

    #[test]
    fn run_coverage_errors_when_lcov_fails() {
        let runner = FakeRunner::new()
            .with_output_fn(|_| Ok(success_output()))
            .with_status_fn(|cmd| {
                if cmd.args.iter().any(|arg| arg == "--lcov") {
                    Ok(status_from_code(1))
                } else {
                    Ok(success_status())
                }
            });
        let _ = runner
            .status(&Cmd::new("cargo").args(["--version"]))
            .unwrap();
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let result = run_coverage(&runner, &out_dir, false, false);
        assert!(result.is_err());
    }

    #[test]
    fn run_coverage_errors_when_html_fails() {
        let runner = FakeRunner::new()
            .with_output_fn(|_| Ok(success_output()))
            .with_status_fn(|cmd| {
                if cmd.args.iter().any(|arg| arg == "--html") {
                    Ok(status_from_code(1))
                } else {
                    Ok(success_status())
                }
            });
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let result = run_coverage(&runner, &out_dir, true, false);
        assert!(result.is_err());
    }

    #[test]
    fn run_coverage_open_branch_executes() {
        let runner = FakeRunner::new()
            .with_output_fn(|_| Ok(success_output()))
            .with_status_fn(|_| Ok(success_status()));
        let temp = TempDir::new().unwrap();
        let out_dir = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        run_coverage(&runner, &out_dir, true, true).unwrap();
    }
}
