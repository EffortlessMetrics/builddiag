//! Helper functions for BDD tests.
//!
//! These helpers are extracted from patterns used in cli_check.rs
//! to provide reusable workspace creation and file writing utilities.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use super::world::{BuilddiagWorld, MsrvConfig, MsrvLocation};

/// Get the path to the builddiag binary.
#[allow(deprecated)]
pub fn get_builddiag_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("builddiag")
}

/// Write a file to the given directory.
pub fn write_file(dir: &Path, rel: &str, contents: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

/// Materialize the workspace based on the World state.
///
/// This function creates all necessary files in the temp directory
/// based on the configuration accumulated from Given steps.
pub fn materialize_workspace(world: &mut BuilddiagWorld) {
    let temp_dir = TempDir::new().expect("failed to create temp directory");
    let dir = temp_dir.path();

    // Build workspace Cargo.toml
    let mut workspace_toml = String::from(
        r#"[workspace]
resolver = "2"
members = ["crates/a""#,
    );

    // Add additional crates to members
    for crate_name in &world.additional_crates {
        workspace_toml.push_str(&format!(r#", "crates/{}""#, crate_name));
    }
    workspace_toml.push_str("]\n");

    // Add workspace.package if we have workspace-level MSRV
    if let Some(ref msrv) = world.msrv {
        if matches!(msrv.location, MsrvLocation::WorkspacePackage) {
            workspace_toml.push_str(&format!(
                r#"
[workspace.package]
rust-version = "{}"
edition = "2021"
"#,
                msrv.version
            ));
        }
    }

    write_file(dir, "Cargo.toml", &workspace_toml);

    // Create main crate
    let crate_toml = build_crate_toml("a", &world.msrv);
    write_file(dir, "crates/a/Cargo.toml", &crate_toml);
    write_file(dir, "crates/a/src/lib.rs", "");

    // Create additional crates
    for crate_name in &world.additional_crates {
        let crate_toml = build_crate_toml(crate_name, &world.msrv);
        write_file(dir, &format!("crates/{}/Cargo.toml", crate_name), &crate_toml);
        write_file(dir, &format!("crates/{}/src/lib.rs", crate_name), "");
    }

    // Create rust-toolchain.toml if configured
    if let Some(ref toolchain) = world.toolchain {
        let toolchain_toml = format!(
            r#"[toolchain]
channel = "{}"
"#,
            toolchain.channel
        );
        write_file(dir, "rust-toolchain.toml", &toolchain_toml);
    }

    // Create checksums file if configured
    if world.has_checksums {
        write_file(dir, "scripts/tools.sha256", "");
    }

    // Write config file if configured
    if let Some(ref content) = world.config_content {
        let config_path = world.config_path.as_deref().unwrap_or(".builddiag.toml");
        write_file(dir, config_path, content);
    }

    // Write any custom files
    for (path, content) in &world.custom_files {
        write_file(dir, path, content);
    }

    world.temp_dir = Some(temp_dir);
}

/// Build a crate Cargo.toml based on MSRV configuration.
fn build_crate_toml(name: &str, msrv: &Option<MsrvConfig>) -> String {
    let mut toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
"#,
        name
    );

    match msrv {
        Some(MsrvConfig {
            location: MsrvLocation::WorkspacePackage,
            ..
        }) => {
            toml.push_str("edition.workspace = true\nrust-version.workspace = true\n");
        }
        Some(MsrvConfig {
            version,
            location: MsrvLocation::CrateOnly,
        }) => {
            toml.push_str(&format!(
                r#"edition = "2021"
rust-version = "{}"
"#,
                version
            ));
        }
        _ => {
            toml.push_str("edition = \"2021\"\n");
        }
    }

    toml
}

/// Run the builddiag check command with the given world configuration.
pub fn run_builddiag_check(world: &mut BuilddiagWorld) {
    let bin = get_builddiag_bin();
    let dir = world.workspace_path();

    let mut cmd = Command::new(&bin);
    cmd.arg("check").arg("--root").arg(&dir).arg("--always");

    // Add profile if specified
    if let Some(ref profile) = world.profile {
        cmd.arg("--profile").arg(profile);
    }

    // Add config file if specified
    if world.config_content.is_some() {
        let config_path = world.config_path.as_deref().unwrap_or(".builddiag.toml");
        cmd.arg("--config").arg(dir.join(config_path));
    }

    // Add extra arguments
    for arg in &world.extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("failed to execute builddiag");
    world.last_output = Some(output);
}

/// Run builddiag with custom arguments (unused but kept for potential future use).
#[allow(dead_code)]
pub fn run_builddiag_with_args(world: &mut BuilddiagWorld, args: &[&str]) {
    let bin = get_builddiag_bin();

    let mut cmd = Command::new(&bin);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("failed to execute builddiag");
    world.last_output = Some(output);
}
