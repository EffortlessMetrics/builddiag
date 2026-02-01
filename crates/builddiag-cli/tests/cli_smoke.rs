use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[allow(deprecated)]
fn get_builddiag_cmd() -> Command {
    Command::cargo_bin("builddiag").unwrap()
}

fn write_file(dir: &TempDir, rel: &str, contents: &str) {
    let p = dir.path().join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

#[test]
fn missing_msrv_fails_by_default() {
    let dir = TempDir::new().unwrap();

    // Minimal workspace with no rust-version.
    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "pub fn f() -> u32 { 1 }\n");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--profile")
        .arg("strict")
        .arg("--always");

    cmd.assert().code(2);
}

#[test]
fn happy_path_passes() {
    let dir = TempDir::new().unwrap();

    write_file(
        &dir,
        "Cargo.toml",
        r#"[workspace]
resolver = "2"
members = ["crates/a"]

[workspace.package]
rust-version = "1.75.0"
edition = "2021"
"#,
    );

    write_file(
        &dir,
        "crates/a/Cargo.toml",
        r#"[package]
name = "a"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
"#,
    );
    write_file(&dir, "crates/a/src/lib.rs", "pub fn f() -> u32 { 1 }\n");

    write_file(
        &dir,
        "rust-toolchain.toml",
        r#"[toolchain]
channel = "1.75.0"
"#,
    );

    // Create empty checksums file (required by default config)
    write_file(&dir, "scripts/tools.sha256", "");

    let mut cmd = get_builddiag_cmd();
    cmd.arg("check")
        .arg("--root")
        .arg(dir.path())
        .arg("--always");

    cmd.assert().success().code(0);
}
