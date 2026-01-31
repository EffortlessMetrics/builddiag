use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use camino::{Utf8PathBuf, Utf8Path};
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
    Schema { #[arg(long, default_value = "schemas")] out_dir: Utf8PathBuf },
    /// Run common CI steps.
    Ci,
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
    run(["cargo", "fmt", "--all", "--", "--check"])?;
    run(["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"])?;
    run(["cargo", "test", "--all"])?;
    run(["cargo", "run", "-p", "xtask", "--", "schema"])?;
    Ok(())
}

fn run(args: impl IntoIterator<Item = &'static str>) -> Result<()> {
    let mut it = args.into_iter();
    let cmd = it.next().unwrap();
    let status = std::process::Command::new(cmd).args(it).status().with_context(|| format!("run {cmd}"))?;
    if !status.success() {
        anyhow::bail!("command failed: {cmd}");
    }
    Ok(())
}
