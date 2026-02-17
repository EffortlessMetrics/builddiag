use anyhow::{Context, Result};
use builddiag_app::{
    compute_changed_files, create_error_receipt, run_check, write_atomic, write_outputs,
};
use builddiag_baseline as baseline;
use builddiag_checks::{BUILTIN_CHECKS, CHECK_DOCS, explain_check};
use builddiag_core::load_config;
use builddiag_domain::{
    exit_code_for,
    explain::{all_check_ids, explain, explain_check_all_codes},
};
use builddiag_fix::{ApplyOptions, FixProposal, apply_fixes, plan_fixes};
use builddiag_hooks::{HookProfile, InitHooksSpec, render_hooks};
use builddiag_render::{render_diagnostics, render_github_annotations, render_markdown};
use builddiag_types::{Config, FailOn, Profile, ProfileCheckState, Report};
use builddiag_watch::{WatchOptions, run_watch_loop};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use std::fmt::Write as _;
use std::io::Write as _;
use std::io::{self};
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;

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

        /// Baseline file path. When provided, only findings not in the baseline
        /// are kept in the emitted report.
        #[arg(long)]
        baseline: Option<Utf8PathBuf>,

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

    /// Continuously run checks when contract files change.
    Watch {
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

        /// Baseline file path for regression-only output.
        #[arg(long)]
        baseline: Option<Utf8PathBuf>,

        /// Annotation output format (github or none).
        #[arg(long, value_enum, default_value = "none")]
        annotations: AnnotationFormat,

        /// Output format for the JSON report.
        #[arg(long, value_enum, default_value = "builddiag")]
        format: OutputFormat,

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

        /// Poll interval in milliseconds.
        #[arg(long, default_value_t = 250)]
        poll_ms: u64,

        /// Debounce window in milliseconds.
        #[arg(long, default_value_t = 300)]
        debounce_ms: u64,

        /// Notify when exit status changes (desktop notification when supported,
        /// terminal bell fallback).
        #[arg(long, default_value_t = false)]
        notify: bool,

        /// Do not clear the terminal between runs.
        #[arg(long, default_value_t = false)]
        no_clear: bool,

        /// Limit runs (primarily for testing and scripted usage).
        #[arg(long, hide = true)]
        max_runs: Option<usize>,
    },

    /// Apply deterministic auto-fixes for unambiguous findings.
    Fix {
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: Utf8PathBuf,

        /// Optional config file (TOML).
        #[arg(long)]
        config: Option<Utf8PathBuf>,

        /// Profile preset (oss, team, strict). Overrides config file profile.
        #[arg(long, value_enum)]
        profile: Option<ProfileArg>,

        /// Print proposed changes without writing files.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Prompt before applying each proposed fix.
        #[arg(long, default_value_t = false)]
        interactive: bool,
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

    /// Generate pre-commit and Git hook snippets for builddiag.
    InitHooks {
        /// Repository root (used for --install).
        #[arg(long, default_value = ".")]
        root: Utf8PathBuf,

        /// Profile used in generated `builddiag check` commands.
        #[arg(long, value_enum, default_value = "oss")]
        profile: ProfileArg,

        /// Generate faster local hook commands (diff-aware, diagnostics, no cache).
        #[arg(long, default_value_t = false)]
        quick_fail: bool,

        /// Output format for generated snippets.
        #[arg(long, value_enum, default_value = "text")]
        format: InitHooksFormat,

        /// Write generated snippets to a file instead of stdout.
        #[arg(long)]
        out: Option<Utf8PathBuf>,

        /// Install the generated shell hook to .git/hooks/pre-commit.
        #[arg(long, default_value_t = false)]
        install: bool,

        /// Overwrite an existing .git/hooks/pre-commit when --install is set.
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Manage finding baselines used for regression-only checks.
    Baseline {
        #[command(subcommand)]
        cmd: BaselineCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BaselineCommand {
    /// Create a new baseline snapshot from current findings.
    Create {
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: Utf8PathBuf,

        /// Optional config file (TOML).
        #[arg(long)]
        config: Option<Utf8PathBuf>,

        /// Profile preset (oss, team, strict). Overrides config file profile.
        #[arg(long, value_enum)]
        profile: Option<ProfileArg>,

        /// Output baseline path (default: <root>/.builddiag-baseline.json).
        #[arg(long)]
        out: Option<Utf8PathBuf>,

        /// Disable caching of repo state (forces full re-parse).
        #[arg(long, default_value_t = false)]
        no_cache: bool,

        /// Custom cache directory (default: .builddiag-cache/).
        #[arg(long)]
        cache_dir: Option<Utf8PathBuf>,
    },

    /// Update an existing baseline by merging current findings.
    Update {
        /// Repository root.
        #[arg(long, default_value = ".")]
        root: Utf8PathBuf,

        /// Optional config file (TOML).
        #[arg(long)]
        config: Option<Utf8PathBuf>,

        /// Profile preset (oss, team, strict). Overrides config file profile.
        #[arg(long, value_enum)]
        profile: Option<ProfileArg>,

        /// Baseline path (default: <root>/.builddiag-baseline.json).
        #[arg(long)]
        out: Option<Utf8PathBuf>,

        /// Disable caching of repo state (forces full re-parse).
        #[arg(long, default_value_t = false)]
        no_cache: bool,

        /// Custom cache directory (default: .builddiag-cache/).
        #[arg(long)]
        cache_dir: Option<Utf8PathBuf>,
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

/// Output format for init-hooks command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum InitHooksFormat {
    /// Human-readable text with code blocks.
    Text,
    /// JSON object with command and snippets.
    Json,
}

#[cfg(test)]
static MAIN_ARGS: Mutex<Option<Vec<std::ffi::OsString>>> = Mutex::new(None);

fn main() -> std::process::ExitCode {
    let cli = Cli::parse_from(main_args());
    let code = run_main(cli);
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

fn run_main(cli: Cli) -> i32 {
    match run_cli(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("builddiag: {e:#}");
            1
        }
    }
}

#[cfg(test)]
fn try_main_from<I>(args: I) -> Result<i32>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let cli = Cli::try_parse_from(args)?;
    run_cli(cli)
}

fn run_cli(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Command::Check {
            root,
            config,
            profile,
            out,
            md,
            baseline,
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
        } => run_check_cli(
            &root,
            config.as_deref(),
            profile,
            out.as_deref(),
            md.as_deref(),
            baseline.as_deref(),
            artifacts_dir.as_deref(),
            annotations,
            format,
            mode,
            diff_aware,
            base.as_deref(),
            head.as_deref(),
            always,
            no_cache,
            cache_dir.as_deref(),
        ),
        Command::Watch {
            root,
            config,
            profile,
            out,
            md,
            baseline,
            annotations,
            format,
            diff_aware,
            base,
            head,
            always,
            no_cache,
            cache_dir,
            poll_ms,
            debounce_ms,
            notify,
            no_clear,
            max_runs,
        } => run_watch_cli(
            &root,
            config.as_deref(),
            profile,
            out.as_deref(),
            md.as_deref(),
            baseline.as_deref(),
            annotations,
            format,
            diff_aware,
            base.as_deref(),
            head.as_deref(),
            always,
            no_cache,
            cache_dir.as_deref(),
            poll_ms,
            debounce_ms,
            notify,
            no_clear,
            max_runs,
        ),
        Command::Fix {
            root,
            config,
            profile,
            dry_run,
            interactive,
        } => run_fix_cli(&root, config.as_deref(), profile, dry_run, interactive),
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
            Ok(0)
        }
        Command::Annotations { report } => {
            let bytes = std::fs::read(&report).with_context(|| format!("read {report}"))?;
            let report: builddiag_types::Report =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {report}"))?;
            for line in render_github_annotations(&report) {
                println!("{line}");
            }
            Ok(0)
        }
        Command::Explain { check_or_code } => match run_explain(&check_or_code) {
            Ok(()) => Ok(0),
            Err(err) => {
                eprintln!("{err}");
                Ok(1)
            }
        },
        Command::ListChecks { profile, format } => {
            list_checks(profile.map(Profile::from), format);
            Ok(0)
        }
        Command::InitHooks {
            root,
            profile,
            quick_fail,
            format,
            out,
            install,
            force,
        } => run_init_hooks_cli(
            &root,
            profile,
            quick_fail,
            format,
            out.as_deref(),
            install,
            force,
        ),
        Command::Baseline { cmd } => run_baseline_cli(cmd),
    }
}

fn run_explain(check_or_code: &str) -> Result<()> {
    match explain_output(check_or_code) {
        Ok(output) => {
            print!("{output}");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn explain_output(check_or_code: &str) -> Result<String> {
    let mut out = String::new();

    // Check if this is a check ID (contains a dot like "rust.msrv_defined")
    if check_or_code.contains('.') {
        // Show all codes for this check
        let entries = explain_check_all_codes(check_or_code);
        if entries.is_empty() {
            // Fall back to old CHECK_DOCS for backward compatibility
            if let Some(doc) = explain_check(check_or_code) {
                write_legacy_doc(&mut out, doc)?;
                return Ok(out);
            }
            let mut err = String::new();
            let header = format!("Unknown check: '{}'\n\nAvailable checks:", check_or_code);
            writeln!(&mut err, "{header}")?;
            for id in all_check_ids() {
                writeln!(&mut err, "  - {id}")?;
            }
            writeln!(&mut err, "\nRun 'builddiag list-checks' for a full list.")?;
            return Err(anyhow::anyhow!(err));
        }

        // Print check header
        writeln!(&mut out, "Check: {}", check_or_code)?;
        writeln!(&mut out, "{}", "=".repeat(7 + check_or_code.len()))?;
        writeln!(&mut out)?;

        // Print each code's explanation
        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                writeln!(&mut out)?;
                writeln!(&mut out, "{}", "-".repeat(60))?;
                writeln!(&mut out)?;
            }
            write_explain_entry(&mut out, entry)?;
        }
    } else {
        // This is a finding code, show specific explanation
        if let Some(entry) = explain(check_or_code) {
            writeln!(&mut out, "Check: {} / Code: {}", entry.check_id, entry.code)?;
            let underline = "=".repeat(15 + entry.check_id.len() + entry.code.len());
            writeln!(&mut out, "{underline}")?;
            writeln!(&mut out)?;
            write_explain_entry(&mut out, entry)?;
        } else {
            // Fall back to old CHECK_DOCS
            if let Some(doc) = explain_check(check_or_code) {
                write_legacy_doc(&mut out, doc)?;
                return Ok(out);
            }
            let err = format!(
                "Unknown check or finding code: '{}'\n\nRun 'builddiag list-checks' to see available checks.",
                check_or_code
            );
            return Err(anyhow::anyhow!(err));
        }
    }

    Ok(out)
}

fn write_explain_entry(
    out: &mut String,
    entry: &builddiag_domain::explain::ExplainEntry,
) -> Result<()> {
    writeln!(out, "{}", entry.name)?;
    writeln!(out, "{}", "-".repeat(entry.name.len()))?;
    writeln!(out)?;
    writeln!(out, "Code: {}", entry.code)?;
    writeln!(out)?;
    writeln!(out, "What it means:")?;
    writeln!(out, "  {}", entry.what_it_means.replace('\n', "\n  "))?;
    writeln!(out)?;
    writeln!(out, "Why it matters:")?;
    writeln!(out, "  {}", entry.why_it_matters.replace('\n', "\n  "))?;
    writeln!(out)?;
    writeln!(out, "How to fix:")?;
    writeln!(out, "  {}", entry.how_to_fix.replace('\n', "\n  "))?;

    if !entry.links.is_empty() {
        writeln!(out)?;
        writeln!(out, "Links:")?;
        for link in entry.links {
            writeln!(out, "  - {}", link)?;
        }
    }
    Ok(())
}

fn write_legacy_doc(out: &mut String, doc: &builddiag_checks::CheckDocumentation) -> Result<()> {
    writeln!(out, "{}", doc.name)?;
    writeln!(out, "{}", "=".repeat(doc.name.len()))?;
    writeln!(out)?;
    writeln!(out, "{}", doc.description)?;
    writeln!(out)?;
    writeln!(out, "Help: {}", doc.help)?;
    if let Some(url) = &doc.url {
        writeln!(out, "Documentation: {}", url)?;
    }
    writeln!(out)?;
    writeln!(out, "Finding codes:")?;
    for code in doc.codes {
        writeln!(out, "  - {}", code)?;
    }
    Ok(())
}

fn list_checks(profile_filter: Option<Profile>, format: ListFormat) {
    let output = match format {
        ListFormat::Json => list_checks_json(profile_filter),
        ListFormat::Table => list_checks_table(profile_filter),
    };
    print!("{output}");
}

fn list_checks_json(profile_filter: Option<Profile>) -> String {
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

    let json = serde_json::to_string_pretty(&checks).unwrap();
    format!("{json}\n")
}

fn list_checks_table(profile_filter: Option<Profile>) -> String {
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

    let mut out = String::new();
    writeln!(&mut out, "Available checks:").unwrap();
    writeln!(&mut out).unwrap();

    // Header
    if profile_filter.is_some() {
        writeln!(
            &mut out,
            "{:<35} {:<25} {:>8}",
            "CHECK ID", "NAME", "SEVERITY"
        )
        .unwrap();
        writeln!(&mut out, "{}", "-".repeat(70)).unwrap();
    } else {
        writeln!(
            &mut out,
            "{:<35} {:<25} {:>6} {:>6} {:>6}",
            "CHECK ID", "NAME", "OSS", "TEAM", "STRICT"
        )
        .unwrap();
        writeln!(&mut out, "{}", "-".repeat(82)).unwrap();
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
            writeln!(
                &mut out,
                "{:<35} {:<25} {:>8}",
                def.id,
                name,
                severity_str(state)
            )
            .unwrap();
        } else {
            writeln!(
                &mut out,
                "{:<35} {:<25} {:>6} {:>6} {:>6}",
                def.id,
                name,
                severity_str(Profile::Oss.check_state(def.id)),
                severity_str(Profile::Team.check_state(def.id)),
                severity_str(Profile::Strict.check_state(def.id)),
            )
            .unwrap();
        }
    }

    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "Use 'builddiag explain <check-id>' for detailed documentation."
    )
    .unwrap();
    out
}

#[allow(clippy::too_many_arguments)]
fn run_init_hooks_cli(
    root: &Utf8Path,
    profile: ProfileArg,
    quick_fail: bool,
    format: InitHooksFormat,
    out: Option<&Utf8Path>,
    install: bool,
    force: bool,
) -> Result<i32> {
    let spec = InitHooksSpec {
        profile: hook_profile(profile),
        quick_fail,
    };
    let bundle = render_hooks(spec);

    let rendered = match format {
        InitHooksFormat::Text => render_init_hooks_text(&bundle),
        InitHooksFormat::Json => render_init_hooks_json(&bundle)?,
    };

    if let Some(path) = out {
        write_atomic(path, rendered.as_bytes())?;
        println!("builddiag: wrote hook snippets to {path}");
    } else {
        print!("{rendered}");
    }

    if install {
        let path = install_pre_commit_hook(root, &bundle.shell_hook_script, force)?;
        println!("builddiag: installed pre-commit hook at {path}");
    }

    Ok(0)
}

fn hook_profile(profile: ProfileArg) -> HookProfile {
    match profile {
        ProfileArg::Oss => HookProfile::Oss,
        ProfileArg::Team => HookProfile::Team,
        ProfileArg::Strict => HookProfile::Strict,
    }
}

fn render_init_hooks_text(bundle: &builddiag_hooks::HooksBundle) -> String {
    let mut out = String::new();
    writeln!(&mut out, "# builddiag init-hooks").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "Check command:").unwrap();
    writeln!(&mut out, "  {}", bundle.check_command).unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "pre-commit snippet (.pre-commit-config.yaml):").unwrap();
    writeln!(&mut out, "```yaml").unwrap();
    write!(&mut out, "{}", bundle.pre_commit_yaml_snippet.trim_end()).unwrap();
    writeln!(&mut out, "\n```").unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Git hook script (.git/hooks/pre-commit):").unwrap();
    writeln!(&mut out, "```sh").unwrap();
    write!(&mut out, "{}", bundle.shell_hook_script.trim_end()).unwrap();
    writeln!(&mut out, "\n```").unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "Husky snippet (.husky/pre-commit):").unwrap();
    writeln!(&mut out, "```sh").unwrap();
    write!(&mut out, "{}", bundle.husky_hook_script.trim_end()).unwrap();
    writeln!(&mut out, "\n```").unwrap();

    out
}

fn render_init_hooks_json(bundle: &builddiag_hooks::HooksBundle) -> Result<String> {
    #[derive(serde::Serialize)]
    struct InitHooksJson<'a> {
        check_command: &'a str,
        pre_commit_yaml_snippet: &'a str,
        shell_hook_script: &'a str,
        husky_hook_script: &'a str,
    }

    let json = InitHooksJson {
        check_command: &bundle.check_command,
        pre_commit_yaml_snippet: &bundle.pre_commit_yaml_snippet,
        shell_hook_script: &bundle.shell_hook_script,
        husky_hook_script: &bundle.husky_hook_script,
    };
    Ok(format!("{}\n", serde_json::to_string_pretty(&json)?))
}

fn install_pre_commit_hook(root: &Utf8Path, script: &str, force: bool) -> Result<Utf8PathBuf> {
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return Err(anyhow::anyhow!(
            "{} does not look like a git repository (missing .git/)",
            root
        ));
    }

    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir).with_context(|| format!("create {hooks_dir}"))?;
    let hook_path = hooks_dir.join("pre-commit");

    if hook_path.exists() && !force {
        return Err(anyhow::anyhow!(
            "{} already exists; pass --force to overwrite",
            hook_path
        ));
    }

    write_atomic(&hook_path, script.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&hook_path, perms)
            .with_context(|| format!("set executable permissions on {hook_path}"))?;
    }

    Ok(hook_path)
}

fn run_baseline_cli(cmd: BaselineCommand) -> Result<i32> {
    match cmd {
        BaselineCommand::Create {
            root,
            config,
            profile,
            out,
            no_cache,
            cache_dir,
        } => {
            let out_path = baseline_path_for(&root, out.as_deref());
            let result = run_check_command(
                &root,
                config.as_deref(),
                profile,
                None,
                None,
                None,
                AnnotationFormat::None,
                OutputFormat::Builddiag,
                false,
                None,
                None,
                true,
                no_cache,
                cache_dir.as_deref(),
            )?;

            let baseline = baseline::from_report(&result.run.report);
            baseline::write(&out_path, &baseline)?;
            println!(
                "builddiag: wrote baseline to {} ({} entries)",
                out_path,
                baseline.entries.len()
            );
            Ok(0)
        }
        BaselineCommand::Update {
            root,
            config,
            profile,
            out,
            no_cache,
            cache_dir,
        } => {
            let out_path = baseline_path_for(&root, out.as_deref());
            let mut existing = baseline::read_or_default(&out_path)?;
            let result = run_check_command(
                &root,
                config.as_deref(),
                profile,
                None,
                None,
                None,
                AnnotationFormat::None,
                OutputFormat::Builddiag,
                false,
                None,
                None,
                true,
                no_cache,
                cache_dir.as_deref(),
            )?;

            let added = baseline::merge_report(&mut existing, &result.run.report)?;
            baseline::write(&out_path, &existing)?;
            println!(
                "builddiag: updated baseline at {} ({} added, {} total)",
                out_path,
                added,
                existing.entries.len()
            );
            Ok(0)
        }
    }
}

fn baseline_path_for(root: &Utf8Path, out: Option<&Utf8Path>) -> Utf8PathBuf {
    match out {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(".builddiag-baseline.json"),
    }
}

fn default_report_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("report.json")
}

fn default_md_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("comment.md")
}

#[allow(clippy::too_many_arguments)]
fn run_check_cli(
    root: &Utf8Path,
    config: Option<&Utf8Path>,
    profile: Option<ProfileArg>,
    out: Option<&Utf8Path>,
    md: Option<&Utf8Path>,
    baseline: Option<&Utf8Path>,
    artifacts_dir: Option<&Utf8Path>,
    annotations: AnnotationFormat,
    format: OutputFormat,
    mode: Mode,
    diff_aware: bool,
    base: Option<&str>,
    head: Option<&str>,
    always: bool,
    no_cache: bool,
    cache_dir: Option<&Utf8Path>,
) -> Result<i32> {
    let start = Utc::now();

    // Cockpit mode defaults to artifacts-dir when no output flags are specified
    let artifacts_dir =
        if mode == Mode::Cockpit && artifacts_dir.is_none() && out.is_none() && md.is_none() {
            Some(root.join("artifacts/builddiag"))
        } else {
            artifacts_dir.map(Utf8PathBuf::from)
        };

    // --artifacts-dir forces sensor format and overrides out/md paths
    let (effective_out, effective_md, effective_format) = if let Some(ref dir) = artifacts_dir {
        (
            Some(dir.join("report.json")),
            Some(dir.join("comment.md")),
            OutputFormat::Sensor,
        )
    } else {
        (
            out.map(Utf8PathBuf::from),
            md.map(Utf8PathBuf::from),
            format,
        )
    };

    // In cockpit mode, wrap everything including config loading in error handling
    let result = run_check_command(
        root,
        config,
        profile,
        effective_out.as_deref(),
        effective_md.as_deref(),
        baseline,
        annotations,
        effective_format,
        diff_aware,
        base,
        head,
        always,
        no_cache,
        cache_dir,
    );

    match (result, mode) {
        (Ok(cmd_result), Mode::Standard) => {
            let artifacts = artifacts_dir.as_deref();
            finish_check_output_for(&cmd_result, annotations, effective_format, artifacts)?;
            Ok(cmd_result.run.exit_code)
        }
        (Ok(cmd_result), Mode::Cockpit) => {
            let artifacts = artifacts_dir.as_deref();
            finish_check_output_for(&cmd_result, annotations, effective_format, artifacts)?;
            Ok(0)
        }
        (Err(e), Mode::Standard) => Err(e),
        (Err(e), Mode::Cockpit) => {
            // Create error receipt and try to write it
            // Use artifacts-dir or --out, falling back to default path
            let out_json = if let Some(ref dir) = artifacts_dir {
                dir.join("report.json")
            } else {
                effective_out.unwrap_or_else(|| root.join("artifacts/builddiag/report.json"))
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
                    Ok(0)
                }
                Err(write_err) => {
                    eprintln!("builddiag: catastrophic failure");
                    eprintln!("  Original error: {e:#}");
                    eprintln!("  Could not write error receipt: {write_err:#}");
                    Ok(1)
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_watch_cli(
    root: &Utf8Path,
    config: Option<&Utf8Path>,
    profile: Option<ProfileArg>,
    out: Option<&Utf8Path>,
    md: Option<&Utf8Path>,
    baseline: Option<&Utf8Path>,
    annotations: AnnotationFormat,
    format: OutputFormat,
    diff_aware: bool,
    base: Option<&str>,
    head: Option<&str>,
    always: bool,
    no_cache: bool,
    cache_dir: Option<&Utf8Path>,
    poll_ms: u64,
    debounce_ms: u64,
    notify: bool,
    no_clear: bool,
    max_runs: Option<usize>,
) -> Result<i32> {
    let mut watch = WatchOptions::for_root(root);
    watch.poll_interval = Duration::from_millis(poll_ms.max(1));
    watch.debounce = Duration::from_millis(debounce_ms.max(1));
    watch.clear_screen = !no_clear;
    watch.notify_on_status_change = notify;
    watch.max_runs = max_runs;
    if let Some(path) = config {
        let watched_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            if let Some(utf8_cwd) = Utf8PathBuf::from_path_buf(cwd).ok() {
                utf8_cwd.join(path)
            } else {
                root.join(path)
            }
        } else {
            root.join(path)
        };
        watch.extra_files.insert(watched_path);
    }

    println!(
        "builddiag: watching {} (poll={}ms, debounce={}ms)",
        root,
        poll_ms.max(1),
        debounce_ms.max(1)
    );
    println!("builddiag: press Ctrl+C to stop");

    run_watch_loop(&watch, || {
        let code = run_check_cli(
            root,
            config,
            profile,
            out,
            md,
            baseline,
            None,
            annotations,
            format,
            Mode::Standard,
            diff_aware,
            base,
            head,
            always,
            no_cache,
            cache_dir,
        )?;
        println!("builddiag: watch run finished with exit code {code}");
        Ok(code)
    })
}

fn run_fix_cli(
    root: &Utf8Path,
    config: Option<&Utf8Path>,
    profile: Option<ProfileArg>,
    dry_run: bool,
    interactive: bool,
) -> Result<i32> {
    let mut cfg: Config = load_config(config)?;
    if let Some(profile_arg) = profile {
        cfg.profile = profile_arg.into();
    }

    let plan = plan_fixes(root, &cfg)?;

    if !plan.warnings.is_empty() {
        for warning in &plan.warnings {
            eprintln!("builddiag: warning: {warning}");
        }
    }

    if plan.proposals.is_empty() {
        println!("builddiag: no unambiguous fixes to apply");
        return Ok(0);
    }

    println!("builddiag: planned {} fix(es):", plan.proposals.len());
    for proposal in &plan.proposals {
        println!(
            "  - [{}] {} ({})",
            proposal.kind.as_str(),
            proposal.summary,
            proposal.target
        );
    }

    let result = apply_fixes(
        root,
        &cfg,
        ApplyOptions {
            dry_run,
            interactive,
        },
        prompt_fix_confirmation,
    )?;

    if dry_run {
        println!(
            "builddiag: dry run complete ({} would apply, {} skipped)",
            result.dry_run_actions, result.skipped
        );
    } else {
        println!(
            "builddiag: applied {} fix(es), skipped {}",
            result.applied, result.skipped
        );
    }

    Ok(0)
}

fn prompt_fix_confirmation(proposal: &FixProposal) -> Result<bool> {
    print!(
        "Apply [{}] {} ({})? [y/N]: ",
        proposal.kind.as_str(),
        proposal.summary,
        proposal.target
    );
    io::stdout().flush().context("flush confirmation prompt")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("read confirmation input")?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(matches!(normalized.as_str(), "y" | "yes"))
}

fn finish_check_output_for(
    cmd_result: &CheckCommandResult,
    annotations: AnnotationFormat,
    format: OutputFormat,
    artifacts_dir: Option<&Utf8Path>,
) -> Result<()> {
    finish_check_output(
        &cmd_result.run,
        cmd_result.sensor_report.as_ref(),
        &cmd_result.out_json,
        cmd_result.out_md.as_deref(),
        annotations,
        format,
        artifacts_dir,
    )
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
    baseline_path: Option<&Utf8Path>,
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
    let fail_on = cfg.defaults.fail_on;

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

    let mut result = match format {
        OutputFormat::Builddiag | OutputFormat::Diagnostics => {
            let run = run_check(root, &cfg, always, changed, cache_config.as_ref())?;
            CheckCommandResult {
                run,
                out_json,
                out_md,
                sensor_report: None,
            }
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
            CheckCommandResult {
                run: builddiag_app::CheckRun {
                    report: result.report,
                    markdown: result.markdown,
                    annotations: result.annotations,
                    exit_code: result.exit_code,
                },
                out_json,
                out_md,
                sensor_report: Some(result.sensor_report),
            }
        }
    };

    if let Some(path) = baseline_path {
        apply_baseline(root, path, fail_on, &mut result)?;
    }
    apply_inline_suppressions(root, fail_on, &mut result)?;

    Ok(result)
}

fn apply_baseline(
    root: &Utf8Path,
    baseline_path: &Utf8Path,
    fail_on: FailOn,
    cmd_result: &mut CheckCommandResult,
) -> Result<()> {
    let resolved = baseline_path_for(root, Some(baseline_path));
    let baseline = baseline::read(&resolved)?;
    let filtered = baseline::filter_report(&cmd_result.run.report, &baseline)?;

    cmd_result.run.report = filtered.report;
    set_baseline_metadata(
        &mut cmd_result.run.report,
        &resolved,
        baseline.entries.len(),
        filtered.suppressed,
        filtered.new_findings,
    );
    cmd_result.run.markdown = render_markdown(&cmd_result.run.report);
    cmd_result.run.annotations = render_github_annotations(&cmd_result.run.report);
    cmd_result.run.exit_code = exit_code_for(cmd_result.run.report.verdict, fail_on);

    if let Some(existing_sensor) = cmd_result.sensor_report.take() {
        let capabilities = existing_sensor
            .run
            .as_ref()
            .map(|run| run.capabilities.clone())
            .unwrap_or_default();
        let artifacts = existing_sensor.artifacts;
        let checks = baseline::checks_from_findings(&cmd_result.run.report.findings);
        let mut sensor = builddiag_app::report_to_sensor(
            &cmd_result.run.report,
            &checks,
            capabilities,
            artifacts,
        );
        sensor.verdict.counts.suppressed = filtered.suppressed;
        cmd_result.sensor_report = Some(sensor);
    }

    Ok(())
}

fn apply_inline_suppressions(
    root: &Utf8Path,
    fail_on: FailOn,
    cmd_result: &mut CheckCommandResult,
) -> Result<()> {
    let filtered = baseline::filter_report_inline_suppressions(root, &cmd_result.run.report)?;
    if filtered.suppressed == 0 {
        return Ok(());
    }

    cmd_result.run.report = filtered.report;
    set_inline_suppression_metadata(
        &mut cmd_result.run.report,
        filtered.suppressed,
        filtered.remaining_findings,
    );
    cmd_result.run.markdown = render_markdown(&cmd_result.run.report);
    cmd_result.run.annotations = render_github_annotations(&cmd_result.run.report);
    cmd_result.run.exit_code = exit_code_for(cmd_result.run.report.verdict, fail_on);

    if let Some(existing_sensor) = cmd_result.sensor_report.take() {
        let previously_suppressed = existing_sensor.verdict.counts.suppressed;
        let capabilities = existing_sensor
            .run
            .as_ref()
            .map(|run| run.capabilities.clone())
            .unwrap_or_default();
        let artifacts = existing_sensor.artifacts;
        let checks = baseline::checks_from_findings(&cmd_result.run.report.findings);
        let mut sensor = builddiag_app::report_to_sensor(
            &cmd_result.run.report,
            &checks,
            capabilities,
            artifacts,
        );
        sensor.verdict.counts.suppressed = previously_suppressed + filtered.suppressed;
        cmd_result.sensor_report = Some(sensor);
    }

    Ok(())
}

fn set_baseline_metadata(
    report: &mut Report,
    baseline_path: &Utf8Path,
    baseline_entries: usize,
    suppressed: usize,
    new_findings: usize,
) {
    let baseline_value = serde_json::json!({
        "path": baseline_path.as_str().replace('\\', "/"),
        "entries": baseline_entries,
        "suppressed": suppressed,
        "new": new_findings
    });

    let mut root = match report.data.take() {
        Some(serde_json::Value::Object(obj)) => obj,
        Some(other) => {
            let mut obj = serde_json::Map::new();
            obj.insert("report_data".to_string(), other);
            obj
        }
        None => serde_json::Map::new(),
    };
    root.insert("baseline".to_string(), baseline_value);
    report.data = Some(serde_json::Value::Object(root));
}

fn set_inline_suppression_metadata(
    report: &mut Report,
    suppressed: usize,
    remaining_findings: usize,
) {
    let suppression_value = serde_json::json!({
        "suppressed": suppressed,
        "remaining": remaining_findings
    });

    let mut root = match report.data.take() {
        Some(serde_json::Value::Object(obj)) => obj,
        Some(other) => {
            let mut obj = serde_json::Map::new();
            obj.insert("report_data".to_string(), other);
            obj
        }
        None => serde_json::Map::new(),
    };
    root.insert("inline_suppressions".to_string(), suppression_value);
    report.data = Some(serde_json::Value::Object(root));
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
            md.map(|md_path| write_atomic(md_path, run.markdown.as_bytes()))
                .transpose()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use builddiag_types::{Finding, Location, SensorReport, SensorVerdict, Severity, Verdict};
    use tempfile::TempDir;

    fn create_minimal_repo() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn demo() {}").unwrap();
        (temp, root)
    }

    fn create_fixable_workspace() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();

        std::fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["crates/a"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            r#"[package]
    name = "a"
    version = "0.1.0"
    edition = "2021"
    rust-version = "1.92"
    "#,
        )
        .unwrap();
        std::fs::write(root.join("crates/a/src/lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(
            root.join("rust-toolchain.toml"),
            r#"[toolchain]
    channel = "1.92.0"
    "#,
        )
        .unwrap();
        std::fs::write(
            root.join("scripts/tools.toml"),
            r#"[[tool]]
name = "demo"
files = ["scripts/tool.sh"]
"#,
        )
        .unwrap();
        std::fs::write(root.join("scripts/tool.sh"), "echo demo\n").unwrap();

        (temp, root)
    }

    fn create_git_workspace() -> (TempDir, Utf8PathBuf) {
        let (temp, root) = create_minimal_repo();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        (temp, root)
    }

    fn minimal_report() -> builddiag_types::Report {
        builddiag_types::Report {
            schema: builddiag_app::REPORT_SCHEMA_V1.to_string(),
            tool: None,
            run: None,
            verdict: Verdict::Pass,
            findings: Vec::new(),
            summary: None,
            data: None,
        }
    }

    fn minimal_run() -> builddiag_app::CheckRun {
        builddiag_app::CheckRun {
            report: minimal_report(),
            markdown: "ok".to_string(),
            annotations: vec!["::notice::ok".to_string()],
            exit_code: 0,
        }
    }

    fn write_report(path: &Utf8Path, report: &builddiag_types::Report) {
        let bytes = serde_json::to_vec_pretty(report).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn test_default_paths_use_config_defaults() {
        let cfg = Config::default();
        let root = Utf8Path::new("repo");
        let report = default_report_path(&cfg, root).as_str().replace('\\', "/");
        let md = default_md_path(&cfg, root).as_str().replace('\\', "/");
        assert_eq!(report, "repo/artifacts/builddiag/report.json");
        assert_eq!(md, "repo/artifacts/builddiag/comment.md");
    }

    #[test]
    fn test_profile_arg_conversion() {
        assert_eq!(Profile::from(ProfileArg::Oss), Profile::Oss);
        assert_eq!(Profile::from(ProfileArg::Team), Profile::Team);
        assert_eq!(Profile::from(ProfileArg::Strict), Profile::Strict);
    }

    #[test]
    fn test_finish_check_output_builddiag_writes_files() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");

        let run = minimal_run();
        finish_check_output(
            &run,
            None,
            &out_json,
            Some(&out_md),
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            None,
        )
        .unwrap();

        assert!(out_json.exists());
        assert!(out_md.exists());
    }

    #[test]
    fn test_finish_check_output_sensor_with_artifacts_dir_writes_payload() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let artifacts_dir = root.join("artifacts");
        let out_json = artifacts_dir.join("report.json");
        let out_md = artifacts_dir.join("comment.md");

        let run = minimal_run();
        let sensor = SensorReport {
            schema: builddiag_types::SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: None,
            run: None,
            verdict: SensorVerdict::default(),
            findings: Vec::new(),
            artifacts: vec![builddiag_types::Artifact {
                name: "placeholder".to_string(),
                path: "artifacts/placeholder.txt".to_string(),
                mime_type: None,
            }],
            data: None,
        };

        finish_check_output(
            &run,
            Some(&sensor),
            &out_json,
            Some(&out_md),
            AnnotationFormat::Github,
            OutputFormat::Sensor,
            Some(&artifacts_dir),
        )
        .unwrap();

        let report_txt = std::fs::read_to_string(&out_json).unwrap();
        let report: serde_json::Value = serde_json::from_str(&report_txt).unwrap();
        let artifacts = report.get("artifacts").and_then(|v| v.as_array()).unwrap();
        assert!(
            artifacts
                .iter()
                .any(|a| { a.get("name").and_then(|v| v.as_str()) == Some("payload") })
        );
        assert!(
            artifacts
                .iter()
                .any(|a| { a.get("name").and_then(|v| v.as_str()) == Some("comment") })
        );

        assert!(artifacts_dir.join("extras").join("payload.json").exists());
        assert!(out_md.exists());
    }

    #[test]
    fn test_finish_check_output_diagnostics_writes_outputs() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");

        let mut report = minimal_report();
        report.findings.push(Finding {
            check_id: "test.check".to_string(),
            code: "test_code".to_string(),
            severity: Severity::Warn,
            message: "diagnostic".to_string(),
            location: Some(Location {
                path: "src/lib.rs".to_string(),
                line: Some(1),
                col: Some(1),
            }),
        });

        let run = builddiag_app::CheckRun {
            report,
            markdown: "md".to_string(),
            annotations: Vec::new(),
            exit_code: 0,
        };

        finish_check_output(
            &run,
            None,
            &out_json,
            Some(&out_md),
            AnnotationFormat::None,
            OutputFormat::Diagnostics,
            None,
        )
        .unwrap();

        assert!(out_json.exists());
        assert!(out_md.exists());
    }

    #[test]
    fn test_run_check_cli_cockpit_error_receipt_written() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let bad_config = root.join("bad.toml");
        std::fs::write(&bad_config, "[defaults").unwrap();

        let exit = run_check_cli(
            &root,
            Some(&bad_config),
            None,
            None,
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Sensor,
            Mode::Cockpit,
            false,
            None,
            None,
            false,
            false,
            None,
        )
        .unwrap();

        assert_eq!(exit, 0);
        let receipt_path = root.join("artifacts/builddiag/report.json");
        assert!(receipt_path.exists());
        let receipt_txt = std::fs::read_to_string(&receipt_path).unwrap();
        let receipt: builddiag_types::Report = serde_json::from_str(&receipt_txt).unwrap();
        assert_eq!(receipt.verdict, Verdict::Error);
    }

    #[test]
    fn test_run_check_cli_cockpit_error_with_explicit_out_uses_out_path() {
        let (_temp, root) = create_minimal_repo();
        let bad_config = root.join("bad.toml");
        std::fs::write(&bad_config, "[defaults").unwrap();
        let out_json = root.join("explicit-report.json");

        let exit = run_check_cli(
            &root,
            Some(&bad_config),
            None,
            Some(&out_json),
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            Mode::Cockpit,
            false,
            None,
            None,
            false,
            false,
            None,
        )
        .unwrap();

        assert_eq!(exit, 0);
        assert!(out_json.exists());
    }

    #[test]
    fn test_run_cli_check_dispatches() {
        let (_temp, root) = create_minimal_repo();

        let cli = Cli {
            cmd: Command::Check {
                root,
                config: None,
                profile: None,
                out: None,
                md: None,
                baseline: None,
                artifacts_dir: None,
                annotations: AnnotationFormat::None,
                format: OutputFormat::Builddiag,
                mode: Mode::Standard,
                diff_aware: false,
                base: None,
                head: None,
                always: false,
                no_cache: true,
                cache_dir: None,
            },
        };

        let exit = run_cli(cli).unwrap();
        assert!(exit == 0 || exit == 2);
    }

    #[test]
    fn test_run_cli_md_outputs_to_stdout() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let report_path = root.join("report.json");
        write_report(&report_path, &minimal_report());

        let cli = Cli {
            cmd: Command::Md {
                report: report_path,
                out: None,
            },
        };

        let exit = run_cli(cli).unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn test_run_cli_annotations_prints_lines() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let report_path = root.join("report.json");

        let mut report = minimal_report();
        report.findings.push(Finding {
            check_id: "test.check".to_string(),
            code: "test_code".to_string(),
            severity: Severity::Error,
            message: "oops".to_string(),
            location: Some(Location {
                path: "src/lib.rs".to_string(),
                line: Some(1),
                col: Some(1),
            }),
        });
        write_report(&report_path, &report);

        let cli = Cli {
            cmd: Command::Annotations {
                report: report_path,
            },
        };

        let exit = run_cli(cli).unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn test_run_cli_explain_unknown_returns_one() {
        let cli = Cli {
            cmd: Command::Explain {
                check_or_code: "unknown.check".to_string(),
            },
        };

        let exit = run_cli(cli).unwrap();
        assert_eq!(exit, 1);
    }

    #[test]
    fn test_run_main_exits_with_code_on_success() {
        let (_temp, root) = create_minimal_repo();
        let cli = Cli {
            cmd: Command::Check {
                root,
                config: None,
                profile: None,
                out: None,
                md: None,
                baseline: None,
                artifacts_dir: None,
                annotations: AnnotationFormat::None,
                format: OutputFormat::Builddiag,
                mode: Mode::Standard,
                diff_aware: false,
                base: None,
                head: None,
                always: false,
                no_cache: true,
                cache_dir: None,
            },
        };

        let code = run_main(cli);
        assert!(matches!(code, 0 | 2));
    }

    #[test]
    fn test_run_main_exits_with_code_on_error() {
        let cli = Cli {
            cmd: Command::Md {
                report: Utf8PathBuf::from("missing.json"),
                out: None,
            },
        };

        let code = run_main(cli);
        assert_eq!(code, 1);
    }

    #[test]
    fn test_main_uses_injected_args() {
        let args = vec![
            std::ffi::OsString::from("builddiag"),
            std::ffi::OsString::from("list-checks"),
        ];
        *super::MAIN_ARGS.lock().unwrap() = Some(args);
        let code = main();
        assert_eq!(code, std::process::ExitCode::from(0));
    }

    #[test]
    fn test_main_args_falls_back_to_env() {
        *super::MAIN_ARGS.lock().unwrap() = None;
        let args = main_args();
        assert!(!args.is_empty());
    }

    #[test]
    fn test_write_legacy_doc_includes_url() {
        let doc = builddiag_checks::CheckDocumentation {
            id: "test.check",
            name: "Test Check",
            description: "desc",
            help: "help",
            url: Some("https://example.com/docs"),
            codes: &["code"],
        };
        let mut out = String::new();
        write_legacy_doc(&mut out, &doc).unwrap();
        assert!(out.contains("Documentation: https://example.com/docs"));
    }

    #[test]
    fn test_try_main_from_parses_list_checks() {
        let args = [
            std::ffi::OsString::from("builddiag"),
            std::ffi::OsString::from("list-checks"),
        ];
        let exit = try_main_from(args).unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn test_run_explain_and_list_checks_do_not_panic() {
        let check_output = explain_output("rust.msrv_defined").unwrap();
        assert!(check_output.contains("Check: rust.msrv_defined"));
        let code_output = explain_output("missing_msrv").unwrap();
        assert!(code_output.contains("Code: missing_msrv"));

        let table_output = list_checks_table(None);
        assert!(table_output.contains("Available checks:"));
        let json_output = list_checks_json(Some(Profile::Oss));
        let parsed: serde_json::Value = serde_json::from_str(json_output.trim()).unwrap();
        assert!(!parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_explain_output_legacy_fallback_for_check_id() {
        let output = explain_output("deps.wildcard_version").unwrap();
        assert!(output.contains("No Wildcard Versions"));
        assert!(output.contains("Finding codes:"));
    }

    #[test]
    fn test_explain_output_legacy_fallback_for_code() {
        let output = explain_output("wildcard_version").unwrap();
        assert!(output.contains("No Wildcard Versions"));
        assert!(output.contains("Finding codes:"));
    }

    #[test]
    fn test_explain_output_unknown_entries() {
        let err = explain_output("unknown.check").unwrap_err();
        assert!(format!("{err}").contains("Unknown check"));

        let err = explain_output("unknown_code").unwrap_err();
        assert!(format!("{err}").contains("Unknown check or finding code"));
    }

    #[test]
    fn test_list_checks_table_filters_skipped_profile() {
        let output = list_checks_table(Some(Profile::Oss));
        assert!(!output.contains("tools.checksums_file_exists"));
        assert!(output.contains("rust.msrv_defined"));
    }

    #[test]
    fn test_list_checks_json_contains_profiles() {
        let output = list_checks_json(None);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        let first = &parsed.as_array().unwrap()[0];
        assert!(first.get("profiles").is_some());
    }

    #[test]
    fn test_list_checks_json_filters_skipped_profile() {
        let output = list_checks_json(Some(Profile::Oss));
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        let ids: Vec<&str> = parsed
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.get("id").and_then(|id| id.as_str()))
            .collect();

        assert!(ids.contains(&"rust.msrv_defined"));
        assert!(!ids.contains(&"tools.checksums_file_exists"));
    }

    #[test]
    fn test_run_check_command_builddiag_and_sensor() {
        let (_temp, root) = create_minimal_repo();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");

        let builddiag = run_check_command(
            &root,
            None,
            None,
            Some(&out_json),
            Some(&out_md),
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            false,
            None,
            None,
            false,
            true,
            None,
        )
        .unwrap();
        assert!(builddiag.sensor_report.is_none());
        assert_eq!(builddiag.out_json, out_json);

        let sensor = run_check_command(
            &root,
            None,
            None,
            Some(&out_json),
            Some(&out_md),
            None,
            AnnotationFormat::None,
            OutputFormat::Sensor,
            false,
            None,
            None,
            false,
            true,
            None,
        )
        .unwrap();
        assert!(sensor.sensor_report.is_some());
        assert_eq!(sensor.run.report.schema, builddiag_app::REPORT_SCHEMA_V1);
    }

    #[test]
    fn test_run_check_command_diagnostics() {
        let (_temp, root) = create_minimal_repo();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");

        let diag = run_check_command(
            &root,
            None,
            None,
            Some(&out_json),
            Some(&out_md),
            None,
            AnnotationFormat::None,
            OutputFormat::Diagnostics,
            false,
            None,
            None,
            false,
            true,
            None,
        )
        .unwrap();

        assert!(diag.sensor_report.is_none());
        assert_eq!(diag.out_json, out_json);
        assert_eq!(diag.out_md, Some(out_md));
    }

    #[test]
    fn test_run_check_cli_standard_error_returns_err() {
        let (_temp, root) = create_minimal_repo();
        let bad_config = root.join("bad.toml");
        std::fs::write(&bad_config, "[defaults").unwrap();

        let result = run_check_cli(
            &root,
            Some(&bad_config),
            None,
            None,
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            Mode::Standard,
            false,
            None,
            None,
            false,
            false,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_run_check_cli_standard_success_returns_exit_code() {
        let (_temp, root) = create_minimal_repo();

        let exit = run_check_cli(
            &root,
            None,
            None,
            None,
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            Mode::Standard,
            false,
            None,
            None,
            false,
            true,
            None,
        )
        .unwrap();

        assert!(exit == 0 || exit == 2);
    }

    #[test]
    fn test_run_check_cli_cockpit_success_returns_zero() {
        let (_temp, root) = create_minimal_repo();

        let exit = run_check_cli(
            &root,
            None,
            None,
            None,
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            Mode::Cockpit,
            false,
            None,
            None,
            false,
            true,
            None,
        )
        .unwrap();

        assert_eq!(exit, 0);
    }

    #[test]
    fn test_run_check_cli_cockpit_write_failure_returns_one() {
        let (_temp, root) = create_minimal_repo();
        let bad_config = root.join("bad.toml");
        std::fs::write(&bad_config, "[defaults").unwrap();
        let artifacts_file = root.join("artifacts");
        std::fs::write(&artifacts_file, "not a dir").unwrap();

        let exit = run_check_cli(
            &root,
            Some(&bad_config),
            None,
            None,
            None,
            None,
            Some(&artifacts_file),
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            Mode::Cockpit,
            false,
            None,
            None,
            false,
            false,
            None,
        )
        .unwrap();

        assert_eq!(exit, 1);
    }

    #[test]
    fn test_run_check_command_with_profile_diff_aware_cache_dir() {
        let (_temp, root) = create_minimal_repo();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");
        let cache_dir = root.join("cache");

        let result = run_check_command(
            &root,
            None,
            Some(ProfileArg::Strict),
            Some(&out_json),
            Some(&out_md),
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            true,
            Some("HEAD"),
            Some("HEAD"),
            false,
            false,
            Some(&cache_dir),
        )
        .unwrap();

        assert_eq!(result.out_json, out_json);
    }

    #[test]
    fn test_run_cli_md_annotations_and_explain() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let report_path = root.join("report.json");
        write_report(&report_path, &minimal_report());

        let out_md = root.join("out.md");
        let md_cli = Cli {
            cmd: Command::Md {
                report: report_path.clone(),
                out: Some(out_md.clone()),
            },
        };
        assert_eq!(run_cli(md_cli).unwrap(), 0);
        assert!(out_md.exists());

        let annotations_cli = Cli {
            cmd: Command::Annotations {
                report: report_path.clone(),
            },
        };
        assert_eq!(run_cli(annotations_cli).unwrap(), 0);

        let explain_cli = Cli {
            cmd: Command::Explain {
                check_or_code: "deps.lockfile_present".to_string(),
            },
        };
        assert_eq!(run_cli(explain_cli).unwrap(), 0);
    }

    #[test]
    fn test_list_checks_prints_json() {
        list_checks(Some(Profile::Oss), ListFormat::Json);
    }

    #[test]
    fn test_write_legacy_doc_outputs_fields() {
        let doc = builddiag_checks::CheckDocumentation {
            id: "legacy.check",
            name: "Legacy Check",
            description: "legacy description",
            help: "legacy help",
            url: Some("https://example.com/legacy"),
            codes: &["legacy_code"],
        };
        let mut out = String::new();
        write_legacy_doc(&mut out, &doc).unwrap();
        assert!(out.contains("Legacy Check"));
        assert!(out.contains("Documentation: https://example.com/legacy"));
        assert!(out.contains("legacy_code"));
    }

    #[test]
    fn test_write_legacy_doc_omits_url_when_missing() {
        let doc = builddiag_checks::CheckDocumentation {
            id: "legacy.check",
            name: "Legacy Check",
            description: "legacy description",
            help: "legacy help",
            url: None,
            codes: &["legacy_code"],
        };
        let mut out = String::new();
        write_legacy_doc(&mut out, &doc).unwrap();
        assert!(!out.contains("Documentation:"));
    }

    #[test]
    fn test_explain_output_legacy_fallbacks() {
        let output = explain_output("deps.lockfile_present").unwrap();
        assert!(output.contains("Finding codes:"));

        let output = explain_output("missing_lockfile_for_binary").unwrap();
        assert!(output.contains("Finding codes:"));
    }

    #[test]
    fn test_explain_output_includes_separators_for_multi_code_checks() {
        let output = explain_output("tools.checksums_format").unwrap();
        assert!(output.contains("------------------------------------------------------------"));
    }

    #[test]
    fn test_finish_check_output_sensor_without_artifacts_dir() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let out_json = root.join("report.json");
        let out_md = root.join("comment.md");

        let run = minimal_run();
        let sensor = SensorReport {
            schema: builddiag_types::SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: None,
            run: None,
            verdict: SensorVerdict::default(),
            findings: Vec::new(),
            artifacts: vec![builddiag_types::Artifact {
                name: "placeholder".to_string(),
                path: "artifacts/placeholder.txt".to_string(),
                mime_type: None,
            }],
            data: None,
        };

        finish_check_output(
            &run,
            Some(&sensor),
            &out_json,
            Some(&out_md),
            AnnotationFormat::None,
            OutputFormat::Sensor,
            None,
        )
        .unwrap();

        assert!(out_json.exists());
        assert!(out_md.exists());

        let report_txt = std::fs::read_to_string(&out_json).unwrap();
        let report: serde_json::Value = serde_json::from_str(&report_txt).unwrap();
        let artifacts = report
            .get("artifacts")
            .and_then(|v| v.as_array())
            .expect("artifacts should be serialized");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts
                .first()
                .and_then(|entry| entry.get("name"))
                .and_then(|v| v.as_str()),
            Some("placeholder")
        );
        assert!(!root.join("extras").join("payload.json").exists());
    }

    #[test]
    fn test_run_cli_list_checks_returns_zero() {
        let cli = Cli {
            cmd: Command::ListChecks {
                profile: None,
                format: ListFormat::Table,
            },
        };
        assert_eq!(run_cli(cli).unwrap(), 0);
    }

    #[test]
    fn test_run_cli_init_hooks_writes_json_file() {
        let (_temp, root) = create_git_workspace();
        let out = root.join("hooks.json");

        let cli = Cli {
            cmd: Command::InitHooks {
                root: root.clone(),
                profile: ProfileArg::Team,
                quick_fail: true,
                format: InitHooksFormat::Json,
                out: Some(out.clone()),
                install: false,
                force: false,
            },
        };

        assert_eq!(run_cli(cli).unwrap(), 0);
        let text = std::fs::read_to_string(out).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let command = json
            .get("check_command")
            .and_then(|v| v.as_str())
            .expect("check_command should be present");
        assert!(command.contains("--profile team"));
        assert!(command.contains("--diff-aware"));
    }

    #[test]
    fn test_run_cli_init_hooks_install_writes_pre_commit_hook() {
        let (_temp, root) = create_git_workspace();
        let cli = Cli {
            cmd: Command::InitHooks {
                root: root.clone(),
                profile: ProfileArg::Strict,
                quick_fail: false,
                format: InitHooksFormat::Text,
                out: None,
                install: true,
                force: false,
            },
        };

        assert_eq!(run_cli(cli).unwrap(), 0);
        let hook = root.join(".git/hooks/pre-commit");
        assert!(hook.exists());
        let content = std::fs::read_to_string(hook).unwrap();
        assert!(content.contains("builddiag check --root . --profile strict"));
    }

    #[test]
    fn test_install_pre_commit_hook_requires_force_for_overwrite() {
        let (_temp, root) = create_git_workspace();
        let hook_path = root.join(".git/hooks/pre-commit");
        std::fs::write(&hook_path, "existing").unwrap();

        let err = install_pre_commit_hook(&root, "#!/bin/sh\n", false).unwrap_err();
        assert!(format!("{err:#}").contains("already exists"));

        install_pre_commit_hook(&root, "#!/bin/sh\necho updated\n", true).unwrap();
        let updated = std::fs::read_to_string(&hook_path).unwrap();
        assert!(updated.contains("updated"));
    }

    #[test]
    fn test_run_fix_cli_applies_changes() {
        let (_temp, root) = create_fixable_workspace();
        let exit = run_fix_cli(&root, None, None, false, false).unwrap();
        assert_eq!(exit, 0);

        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("resolver = \"2\""));
        assert!(manifest.contains("rust-version = \"1.92.0\""));
        assert!(root.join("scripts/tools.sha256").exists());
    }

    #[test]
    fn test_run_fix_cli_dry_run_leaves_files_unchanged() {
        let (_temp, root) = create_fixable_workspace();
        let before_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();

        let exit = run_fix_cli(&root, None, None, true, false).unwrap();
        assert_eq!(exit, 0);

        let after_manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert_eq!(before_manifest, after_manifest);
        assert!(!root.join("scripts/tools.sha256").exists());
    }

    #[test]
    fn test_run_watch_cli_with_max_runs_exits() {
        let (_temp, root) = create_minimal_repo();
        let exit = run_watch_cli(
            &root,
            None,
            None,
            None,
            None,
            None,
            AnnotationFormat::None,
            OutputFormat::Builddiag,
            false,
            None,
            None,
            false,
            true,
            None,
            10,
            10,
            false,
            true,
            Some(1),
        )
        .unwrap();

        assert!(exit == 0 || exit == 2);
    }
}
