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
    }
}

fn generate_schemas(out_dir: &Utf8Path) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {out_dir}"))?;

    let report = schema_for!(builddiag_types::Report);
    let cfg = schema_for!(builddiag_types::Config);

    write_json(out_dir.join("builddiag.report.v1.schema.json"), &report)?;
    write_json(out_dir.join("builddiag.config.v1.schema.json"), &cfg)?;

    Ok(())
}

fn write_json(path: Utf8PathBuf, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(&path, bytes).with_context(|| format!("write {path}"))?;
    Ok(())
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
