//! Repository-related test fixtures and generators.

/// Helper to create a minimal Cargo.toml for a package.
pub fn make_package_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#
    )
}

/// Helper to create a workspace Cargo.toml with members.
pub fn make_workspace_toml(members: &[&str]) -> String {
    let members_str: Vec<String> = members.iter().map(|m| format!("\"{}\"", m)).collect();
    format!(
        r#"[workspace]
resolver = "2"
members = [
    {}
]
"#,
        members_str.join(",\n    ")
    )
}

/// Returns `true` if the name is a Windows reserved device name.
pub fn is_windows_reserved(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}
