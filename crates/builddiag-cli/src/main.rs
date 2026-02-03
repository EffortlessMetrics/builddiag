use anyhow::{Context, Result};
use builddiag_app::{compute_changed_files, load_config, run_check, write_outputs};
use builddiag_checks::{BUILTIN_CHECKS, CHECK_DOCS, explain_check};
use builddiag_domain::explain::{all_check_ids, explain, explain_check_all_codes};
use builddiag_render::{render_github_annotations, render_markdown};
use builddiag_types::{Config, Profile, ProfileCheckState};
use camino::{Utf8Path, Utf8PathBuf};
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

        /// Annotation output format (github or none).
        #[arg(long, value_enum, default_value = "none")]
        annotations: AnnotationFormat,

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
            annotations,
            diff_aware,
            base,
            head,
            always,
        } => {
            let cfg_path = config.as_deref();
            let mut cfg: Config = load_config(cfg_path)?;

            // CLI --profile overrides config file profile
            if let Some(profile_arg) = profile {
                cfg.profile = profile_arg.into();
            }

            if diff_aware {
                cfg.defaults.diff_aware = true;
            }

            let base = base.unwrap_or_else(|| cfg.defaults.base.clone());
            let head = head.unwrap_or_else(|| cfg.defaults.head.clone());

            let changed = if cfg.defaults.diff_aware {
                compute_changed_files(&root, &base, &head)?
            } else {
                None
            };

            let run = run_check(&root, &cfg, always, changed)?;

            let out_json = out.unwrap_or_else(|| default_report_path(&cfg, &root));
            let out_md = md.or_else(|| Some(default_md_path(&cfg, &root)));
            write_outputs(&out_json, out_md.as_deref(), &run)?;

            if annotations == AnnotationFormat::Github {
                for line in &run.annotations {
                    println!("{line}");
                }
            }

            process::exit(run.exit_code);
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
