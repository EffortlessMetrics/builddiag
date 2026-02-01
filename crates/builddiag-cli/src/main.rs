use anyhow::{Context, Result};
use builddiag_app::{compute_changed_files, load_config, run_check, write_outputs};
use builddiag_checks::explain_check;
use builddiag_render::{render_github_annotations, render_markdown};
use builddiag_types::{Config, Profile};
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

        /// Emit GitHub Actions annotations to stdout.
        #[arg(long, default_value_t = false)]
        github_annotations: bool,

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
    GithubAnnotations {
        #[arg(long)]
        report: Utf8PathBuf,
    },

    /// Show documentation for a check or finding code.
    Explain {
        /// Check ID (e.g., "rust.msrv_defined") or finding code (e.g., "missing_msrv").
        check_or_code: String,
    },
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
            github_annotations,
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

            if github_annotations {
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
        Command::GithubAnnotations { report } => {
            let bytes = std::fs::read(&report).with_context(|| format!("read {report}"))?;
            let report: builddiag_types::Report =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {report}"))?;
            for line in render_github_annotations(&report) {
                println!("{line}");
            }
        }
        Command::Explain { check_or_code } => {
            if let Some(doc) = explain_check(&check_or_code) {
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
            } else {
                eprintln!(
                    "Unknown check or finding code: '{}'\n\nRun 'builddiag explain --help' for usage.",
                    check_or_code
                );
                process::exit(1);
            }
        }
    }

    Ok(())
}

fn default_report_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("report.json")
}

fn default_md_path(cfg: &Config, root: &Utf8Path) -> Utf8PathBuf {
    root.join(&cfg.defaults.out_dir).join("comment.md")
}
