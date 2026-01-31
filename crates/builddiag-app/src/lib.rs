use anyhow::{anyhow, Context, Result};
use builddiag_checks::run_selected_checks;
use builddiag_domain::{exit_code_for, summarize};
use builddiag_render::{render_github_annotations, render_markdown};
use builddiag_repo::load_repo_state;
use builddiag_types::{Config, Inputs, RepoDetected, RepoInfo, Report, SchemaId, ToolInfo};
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

pub fn compute_changed_files(root: &Utf8Path, base: &str, head: &str) -> Result<Option<BTreeSet<String>>> {
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

pub fn run_check(root: &Utf8Path, config: &Config, allow_all: bool, changed_files: Option<BTreeSet<String>>) -> Result<CheckRun> {
    let start = Utc::now();

    let repo_state = load_repo_state(root, config, changed_files)?;

    let checks = run_selected_checks(&repo_state, config, allow_all)?;
    let summary = summarize(&checks);

    let inputs = Inputs {
        cargo_root: repo_state.cargo_root.as_ref().map(|p| rel(&repo_state.root, p)),
        rust_toolchain: repo_state.toolchain.as_ref().map(|t| rel(&repo_state.root, &t.path)),
        tools_checksums: repo_state.tools_checksums.as_ref().map(|t| rel(&repo_state.root, &t.path)),
        tools_manifest: repo_state.tools_manifest.as_ref().map(|(p, _)| rel(&repo_state.root, p)),
    };

    let report = Report {
        schema: SchemaId(REPORT_SCHEMA_V1.to_string()),
        tool: ToolInfo {
            name: "builddiag".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: builddiag_types::RunInfo {
            id: start.to_rfc3339(),
            started_at: start,
            ended_at: Some(Utc::now()),
        },
        repo: RepoInfo {
            root: repo_state.root.to_string(),
            detected: RepoDetected {
                is_workspace: repo_state.workspace.is_workspace,
                members: repo_state.workspace.members.len(),
            },
        },
        inputs,
        checks: checks.clone(),
        summary: summary.clone(),
    };

    let markdown = render_markdown(&report);
    let annotations = render_github_annotations(&report);
    let exit_code = exit_code_for(&summary, config.defaults.fail_on);

    Ok(CheckRun {
        report,
        markdown,
        annotations,
        exit_code,
    })
}

pub fn write_atomic(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| anyhow!("no parent dir for {path}"))?;
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

fn rel(root: &Utf8Path, p: &Utf8Path) -> String {
    p.strip_prefix(root).ok().unwrap_or(p).to_string()
}
