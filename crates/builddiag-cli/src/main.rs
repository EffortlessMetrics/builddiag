use anyhow::{Context, Result};
use builddiag_app::{
    compute_changed_files, create_error_receipt, run_check, write_atomic, write_outputs,
};
use builddiag_checks::{BUILTIN_CHECKS, CHECK_DOCS, explain_check};
use builddiag_core::load_config;
use builddiag_domain::explain::{all_check_ids, explain, explain_check_all_codes};
use builddiag_render::{render_diagnostics, render_github_annotations, render_markdown};
use builddiag_types::{Config, Profile, ProfileCheckState};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use std::process;

/// CLI-compatible profile enum for clap value parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProfileArg {
    /// Open source profile with warn-heavy defaults.
    Oss,
    /// Team profile with stronger gating.
    Team,
    /// Strict profile with maximum enforcement.
    Strict,
}

/// Annotation output format for the check command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum AnnotationFormat {
    /// Output GitHub Actions workflow annotations.
    Github,
    /// No annotation output.
    #[default]
    None,
}

/// Output format for the JSON report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum OutputFormat {
    /// Native builddiag.report.v1 format (default).
    #[default]
    Builddiag,
    /// Cockpit-compatible sensor.report.v1 format.
    Sensor,
    /// IDE-compatible diagnostic lines (path:line:col: severity: message).
    Diagnostics,
}

/// Exit code mode for the check command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum Mode {
    /// Standard mode: exit 0/1/2 based on verdict.
    /// - 0: Pass or Warn (when fail_on=error)
    /// - 1: Runtime error
    /// - 2: Policy violation
    #[default]
    Standard,
    /// Cockpit CI mode: exit 0 if report written successfully.
    /// - 0: Report written (regardless of verdict)
    /// - 1: Catastrophic failure (could not write report)
    Cockpit,
}

impl From<ProfileArg> for Profile {
    fn from(arg: ProfileArg) -> Self {
        match arg {
            ProfileArg::Oss => Profile::Oss,
            ProfileArg::Team => Profile::Team,
            ProfileArg::Strict => Profile::Strict,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "builddiag",
    version,
    about = "Check the build contract of a Rust repository"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run build contract checks and write a JSON report (and optionally Markdown).
    Check {
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: Utf8PathBuf,

        /// Optional config file (TOML).
        #[arg(long)]
        config: Option<Utf8PathBuf>,

        /// Profile preset (oss, team, strict). Overrides config file profile.
        #[arg(long, value_enum)]
        profile: Option<ProfileArg>,

        /// Output JSON report path.
        #[arg(long)]
        out: Option<Utf8PathBuf>,

        /// Output Markdown summary path.
        #[arg(long)]
        md: Option<Utf8PathBuf>,

        /// Artifacts output directory. When set, always writes sensor.report.v1
        /// to <dir>/report.json with builddiag-native payload in <dir>/extras/payload.json.
        /// Overrides --out, --md, and --format.
        #[arg(long)]
        artifacts_dir: Option<Utf8PathBuf>,

        /// Annotation output format (github or none).
        #[arg(long, value_enum, default_value = "none")]
        annotations: AnnotationFormat,

        /// Output format for the JSON report.
        #[arg(long, value_enum, default_value = "builddiag")]
        format: OutputFormat,

        /// Exit code mode.
        #[arg(long, value_enum, default_value = "standard")]
        mode: Mode,

        /// Enable diff-aware skipping (only run checks triggered by changed files).
        #[arg(long, default_value_t = false)]
        diff_aware: bool,

        /// Base git ref for diff-aware mode.
        #[arg(long)]
        base: Option<String>,

        /// Head git ref for diff-aware mode.
        #[arg(long)]
        head: Option<String>,

        /// Run all checks even when diff-aware.
        #[arg(long, default_value_t = false)]
        always: bool,

        /// Disable caching of repo state (forces full re-parse).
        #[arg(long, default_value_t = false)]
        no_cache: bool,

        /// Custom cache directory (default: .builddiag-cache/).
        #[arg(long)]
        cache_dir: Option<Utf8PathBuf>,
    },

    /// Render Markdown from an existing JSON report.
    Md {
        #[arg(long)]
        report: Utf8PathBuf,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
    },

    /// Emit GitHub Actions annotations from an existing report.
    #[command(alias = "github-annotations")]
    Annotations {
        /// Path to the JSON report file.
        #[arg(long)]
        report: Utf8PathBuf,
    },

    /// Show documentation for a check or finding code.
    Explain {
        /// Check ID (e.g., "rust.msrv_defined") or finding code (e.g., "missing_msrv").
        check_or_code: String,
    },

    /// List all available checks with their profile severities.
    ListChecks {
        /// Profile to show (defaults to showing all profiles).
        #[arg(long, value_enum)]
        profile: Option<ProfileArg>,

        /// Output format: table (default) or json.
        #[arg(long, default_value = "table")]
        format: ListFormat,
    },
}

/// Output format for list-checks command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ListFormat {
    /// Human-readable table format.
    Table,
    /// JSON format for machine processing.
    Json,
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("builddiag: {e:#}");
        process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Check {
            root,
            config,
            profile,
            out,
            md,
            artifacts_dir,
            annotations,
            format,
            mode,
            diff_aware,
            base,
            head,
            always,
            no_cache,
            cache_dir,
        } => {
            let start = Utc::now();

            // --artifacts-dir forces sensor format and overrides out/md paths
            let (effective_out, effective_md, effective_format) =
                if let Some(ref dir) = artifacts_dir {
                    (
                        Some(dir.join("report.json")),
                        Some(dir.join("comment.md")),
                        OutputFormat::Sensor,
                    )
                } else {
                    (out.clone(), md.clone(), format)
                };

            // In cockpit mode, wrap everything including config loading in error handling
            let result = run_check_command(
                &root,
                config.as_deref(),
                profile,
                effective_out.as_deref(),
                effective_md.as_deref(),
                annotations,
                effective_format,
                diff_aware,
                base.as_deref(),
                head.as_deref(),
                always,
                no_cache,
                cache_dir.as_deref(),
            );

            match (result, mode) {
                (Ok(cmd_result), Mode::Standard) => {
                    // Write outputs
                    finish_check_output(
                        &cmd_result.run,
                        cmd_result.sensor_report.as_ref(),
                        &cmd_result.out_json,
                        cmd_result.out_md.as_deref(),
                        annotations,
                        effective_format,
                        artifacts_dir.as_deref(),
                    )?;
                    process::exit(cmd_result.run.exit_code);
                }
                (Ok(cmd_result), Mode::Cockpit) => {
                    // Write outputs
                    finish_check_output(
                        &cmd_result.run,
                        cmd_result.sensor_report.as_ref(),
                        &cmd_result.out_json,
                        cmd_result.out_md.as_deref(),
                        annotations,
                        effective_format,
                        artifacts_dir.as_deref(),
                    )?;
                    process::exit(0); // Report written successfully
                }
                (Err(e), Mode::Standard) => {
                    return Err(e);
                }
                (Err(e), Mode::Cockpit) => {
                    // Create error receipt and try to write it
                    // Use artifacts-dir or --out, falling back to default path
                    let out_json = if let Some(ref dir) = artifacts_dir {
                        dir.join("report.json")
                    } else {
                        effective_out
                            .unwrap_or_else(|| root.join("artifacts/builddiag/report.json"))
                    };

                    let receipt = create_error_receipt(start, &e);
                    let json = serde_json::to_vec_pretty(&receipt)?;

                    // Ensure parent directory exists
                    if let Some(parent) = out_json.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }

                    match write_atomic(&out_json, &json) {
                        Ok(()) => {
                            eprintln!(
                                "builddiag: internal error occurred, error receipt written to {}",
                                out_json
                            );
                            eprintln!("  Error: {e:#}");
                            process::exit(0); // Report written (even if error)
                        }
                        Err(write_err) => {
                            eprintln!("builddiag: catastrophic failure");
                            eprintln!("  Original error: {e:#}");
                            eprintln!("  Could not write error receipt: {write_err:#}");
                            process::exit(1);
                        }
                    }
                }
            }
        }
        Command::Md { report, out } => {
            let bytes = std::fs::read(&report).with_context(|| format!("read {report}"))?;
            let report: builddiag_types::Report =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {report}"))?;
            let md = render_markdown(&report);
            if let Some(out) = out {
                builddiag_app::write_atomic(&out, md.as_bytes())?;
            } else {
                print!("{md}");
            }
        }
        Command::Annotations { report } => {
            let bytes = std::fs::read(&report).with_context(|| format!("read {report}"))?;
            let report: builddiag_types::Report =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {report}"))?;
            for line in render_github_annotations(&report) {
                println!("{line}");
            }
        }
        Command::Explain { check_or_code } => {
            run_explain(&check_or_code);
        }
        Command::ListChecks { profile, format } => {
            list_checks(profile.map(Profile::from), format);
        }
    }

    Ok(())
}

fn run_explain(check_or_code: &str) {
    // Check if this is a check ID (contains a dot like "rust.msrv_defined")
    if check_or_code.contains('.') {
        // Show all codes for this check
        let entries = explain_check_all_codes(check_or_code);
        if entries.is_empty() {
            // Fall back to old CHECK_DOCS for backward compatibility
            if let Some(doc) = explain_check(check_or_code) {
                print_legacy_doc(doc);
                return;
            }
            eprintln!("Unknown check: '{}'\n\nAvailable checks:", check_or_code);
            for id in all_check_ids() {
                eprintln!("  - {}", id);
            }
            eprintln!("\nRun 'builddiag list-checks' for a full list.");
            process::exit(1);
        }

        // Print check header
        println!("Check: {}", check_or_code);
        println!("{}", "=".repeat(7 + check_or_code.len()));
        println!();

        // Print each code's explanation
        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                println!();
                println!("{}", "-".repeat(60));
                println!();
            }
            print_explain_entry(entry);
        }
    } else {
        // This is a finding code, show specific explanation
        if let Some(entry) = explain(check_or_code) {
            println!("Check: {} / Code: {}", entry.check_id, entry.code);
            println!(
                "{}",
                "=".repeat(15 + entry.check_id.len() + entry.code.len())
            );
            println!();
            print_explain_entry(entry);
        } else {
            // Fall back to old CHECK_DOCS
            if let Some(doc) = explain_check(check_or_code) {
                print_legacy_doc(doc);
                return;
            }
            eprintln!(
                "Unknown check or finding code: '{}'\n\nRun 'builddiag list-checks' to see available checks.",
                check_or_code
            );
            process::exit(1);
        }
    }
}

fn print_explain_entry(entry: &builddiag_domain::explain::ExplainEntry) {
    println!("{}", entry.name);
    println!("{}", "-".repeat(entry.name.len()));
    println!();
    println!("Code: {}", entry.code);
    println!();
    println!("What it means:");
    println!("  {}", entry.what_it_means.replace('\n', "\n  "));
    println!();
    println!("Why it matters:");
    println!("  {}", entry.why_it_matters.replace('\n', "\n  "));
    println!();
    println!("How to fix:");
    println!("  {}", entry.how_to_fix.replace('\n', "\n  "));

    if !entry.links.is_empty() {
        println!();
        println!("Links:");
        for link in entry.links {
            println!("  - {}", link);
        }
    }
}

fn print_legacy_doc(doc: &builddiag_checks::CheckDocumentation) {
    println!("{}", doc.name);
    println!("{}", "=".repeat(doc.name.len()));
    println!();
    println!("{}", doc.description);
    println!();
    println!("Help: {}", doc.help);
    if let Some(url) = &doc.url {
        println!("Documentation: {}", url);
    }
    println!();
    println!("Finding codes:");
    for code in doc.codes {
        println!("  - {}", code);
    }
}

fn list_checks(profile_filter: Option<Profile>, format: ListFormat) {
    match format {
        ListFormat::Json => list_checks_json(profile_filter),
        ListFormat::Table => list_checks_table(profile_filter),
    }
}

fn list_checks_json(profile_filter: Option<Profile>) {
    #[derive(serde::Serialize)]
    struct CheckInfo {
        id: &'static str,
        name: &'static str,
        description: &'static str,
        codes: &'static [&'static str],
        profiles: ProfileInfo,
    }

    #[derive(serde::Serialize)]
    struct ProfileInfo {
        oss: ProfileState,
        team: ProfileState,
        strict: ProfileState,
    }

    #[derive(serde::Serialize)]
    struct ProfileState {
        enabled: bool,
        severity: Option<&'static str>,
    }

    fn profile_state_to_json(state: ProfileCheckState) -> ProfileState {
        match state {
            ProfileCheckState::Skip => ProfileState {
                enabled: false,
                severity: None,
            },
            ProfileCheckState::Enabled(sev) => ProfileState {
                enabled: true,
                severity: Some(match sev {
                    builddiag_types::Severity::Info => "info",
                    builddiag_types::Severity::Warn => "warn",
                    builddiag_types::Severity::Error => "error",
                }),
            },
        }
    }

    let checks: Vec<CheckInfo> = BUILTIN_CHECKS
        .iter()
        .filter_map(|def| {
            // Find documentation for this check
            let doc = CHECK_DOCS.iter().find(|d| d.id == def.id)?;

            // If filtering by profile, skip checks that are disabled in that profile
            if let Some(p) = profile_filter
                && matches!(p.check_state(def.id), ProfileCheckState::Skip)
            {
                return None;
            }

            Some(CheckInfo {
                id: def.id,
                name: doc.name,
                description: doc.description,
                codes: doc.codes,
                profiles: ProfileInfo {
                    oss: profile_state_to_json(Profile::Oss.check_state(def.id)),
                    team: profile_state_to_json(Profile::Team.check_state(def.id)),
                    strict: profile_state_to_json(Profile::Strict.check_state(def.id)),
                },
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&checks).unwrap());
}

fn list_checks_table(profile_filter: Option<Profile>) {
    fn severity_str(state: ProfileCheckState) -> &'static str {
        match state {
            ProfileCheckState::Skip => "skip",
            ProfileCheckState::Enabled(sev) => match sev {
                builddiag_types::Severity::Info => "info",
                builddiag_types::Severity::Warn => "warn",
                builddiag_types::Severity::Error => "error",
            },
        }
    }

    println!("Available checks:");
    println!();

    // Header
    if profile_filter.is_some() {
        println!("{:<35} {:<25} {:>8}", "CHECK ID", "NAME", "SEVERITY");
        println!("{}", "-".repeat(70));
    } else {
        println!(
            "{:<35} {:<25} {:>6} {:>6} {:>6}",
            "CHECK ID", "NAME", "OSS", "TEAM", "STRICT"
        );
        println!("{}", "-".repeat(82));
    }

    for def in BUILTIN_CHECKS {
        let doc = CHECK_DOCS.iter().find(|d| d.id == def.id);
        let name = doc.map(|d| d.name).unwrap_or("(unknown)");

        // If filtering by profile, skip checks that are disabled
        if let Some(p) = profile_filter {
            let state = p.check_state(def.id);
            if matches!(state, ProfileCheckState::Skip) {
                continue;
            }
            println!("{:<35} {:<25} {:>8}", def.id, name, severity_str(state));
        } else {
            println!(
                "{:<35} {:<25} {:>6} {:>6} {:>6}",
                def.id,
                name,
                severity_str(Profile::Oss.check_state(def.id)),
                severity_str(Profile::Team.check_state(def.id)),
                severity_str(Profile::Strict.check_state(def.id)),
            );
        }
    }

    println!();
    println!("Use 'builddiag explain <check-id>' for detailed documentation.");
}

fn default_report_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("report.json")
}

fn default_md_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("comment.md")
}

/// Result from running the check command, including output paths.
struct CheckCommandResult {
    run: builddiag_app::CheckRun,
    out_json: Utf8PathBuf,
    out_md: Option<Utf8PathBuf>,
    sensor_report: Option<builddiag_types::SensorReport>,
}

/// Run the check command and return the result.
/// This is separated out to allow error handling in cockpit mode.
#[allow(clippy::too_many_arguments)]
fn run_check_command(
    root: &Utf8Path,
    config: Option<&Utf8Path>,
    profile: Option<ProfileArg>,
    out: Option<&Utf8Path>,
    md: Option<&Utf8Path>,
    _annotations: AnnotationFormat,
    format: OutputFormat,
    diff_aware: bool,
    base: Option<&str>,
    head: Option<&str>,
    always: bool,
    no_cache: bool,
    cache_dir: Option<&Utf8Path>,
) -> Result<CheckCommandResult> {
    let mut cfg: Config = load_config(config)?;

    // CLI --profile overrides config file profile
    if let Some(profile_arg) = profile {
        cfg.profile = profile_arg.into();
    }

    if diff_aware {
        cfg.defaults.diff_aware = true;
    }

    // Configure caching
    let cache_config = if no_cache {
        None
    } else {
        let mut cc = builddiag_app::CacheConfig::default();
        if let Some(dir) = cache_dir {
            cc.cache_dir = dir.to_path_buf();
        }
        Some(cc)
    };

    let base = base
        .map(String::from)
        .unwrap_or_else(|| cfg.defaults.base.clone());
    let head = head
        .map(String::from)
        .unwrap_or_else(|| cfg.defaults.head.clone());

    let changed = if cfg.defaults.diff_aware {
        compute_changed_files(root, &base, &head)?
    } else {
        None
    };

    let out_json = out
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|| default_report_path(&cfg, root));
    let out_md = md
        .map(Utf8PathBuf::from)
        .or_else(|| Some(default_md_path(&cfg, root)));

    match format {
        OutputFormat::Builddiag | OutputFormat::Diagnostics => {
            let run = run_check(root, &cfg, always, changed, cache_config.as_ref())?;
            Ok(CheckCommandResult {
                run,
                out_json,
                out_md,
                sensor_report: None,
            })
        }
        OutputFormat::Sensor => {
            let settings = builddiag_core::Settings {
                root: root.to_path_buf(),
                config: cfg,
                allow_all: always,
                changed_files: changed,
                cache_config,
                substrate: None,
            };
            let result = builddiag_core::run(&settings)?;
            Ok(CheckCommandResult {
                run: builddiag_app::CheckRun {
                    report: result.report,
                    markdown: result.markdown,
                    annotations: result.annotations,
                    exit_code: result.exit_code,
                },
                out_json,
                out_md,
                sensor_report: Some(result.sensor_report),
            })
        }
    }
}

/// Write the outputs and print annotations.
fn finish_check_output(
    run: &builddiag_app::CheckRun,
    sensor_report: Option<&builddiag_types::SensorReport>,
    out_json: &Utf8Path,
    md: Option<&Utf8Path>,
    annotations: AnnotationFormat,
    format: OutputFormat,
    artifacts_dir: Option<&Utf8Path>,
) -> Result<()> {
    match format {
        OutputFormat::Builddiag => {
            write_outputs(out_json, md, run)?;
        }
        OutputFormat::Sensor => {
            let mut sensor = sensor_report.expect("sensor report should exist").clone();

            // When using --artifacts-dir, write builddiag-native payload to extras/
            // and reference it from the sensor envelope
            if let Some(dir) = artifacts_dir {
                let extras_dir = dir.join("extras");
                std::fs::create_dir_all(&extras_dir)
                    .with_context(|| format!("create {extras_dir}"))?;
                let payload_path = extras_dir.join("payload.json");
                let payload = serde_json::to_vec_pretty(&run.report)?;
                write_atomic(&payload_path, &payload)?;

                sensor.artifacts.push(builddiag_types::Artifact {
                    name: "payload".to_string(),
                    path: "extras/payload.json".to_string(),
                    mime_type: Some("application/json".to_string()),
                });

                sensor.artifacts.push(builddiag_types::Artifact {
                    name: "comment".to_string(),
                    path: "comment.md".to_string(),
                    mime_type: Some("text/markdown".to_string()),
                });
            }

            let json = serde_json::to_vec_pretty(&sensor)?;
            write_atomic(out_json, &json)?;
            if let Some(md_path) = md {
                write_atomic(md_path, run.markdown.as_bytes())?;
            }
        }
        OutputFormat::Diagnostics => {
            // Diagnostics format outputs to stdout for IDE integration
            // No file writing - just print diagnostic lines
            let lines = render_diagnostics(&run.report);
            for line in lines {
                println!("{line}");
            }
            // Still write the JSON report if explicitly requested
            write_outputs(out_json, md, run)?;
        }
    }

    if annotations == AnnotationFormat::Github {
        for line in &run.annotations {
            println!("{line}");
        }
    }

    Ok(())
}
