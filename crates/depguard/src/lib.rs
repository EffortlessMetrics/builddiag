//! Dependency hygiene sensor for Rust workspaces.
//!
//! depguard validates dependency declarations in Cargo.toml files to catch
//! common issues that can cause publishing failures or maintenance problems:
//!
//! - **Wildcard versions**: `foo = "*"` is fragile and should specify a version
//! - **Path-only dependencies**: Path deps need version for crates.io publishing
//! - **Workspace inheritance**: Members should inherit from workspace when possible
//!
//! # Example
//!
//! ```no_run
//! use depguard::{check_workspace, Config};
//! use camino::Utf8Path;
//!
//! let findings = check_workspace(Utf8Path::new("."), &Config::default()).unwrap();
//! for f in findings {
//!     println!("{}: {}", f.code, f.message);
//! }
//! ```

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Severity level for findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational finding, does not affect pass/fail.
    Info,
    /// Warning finding.
    #[default]
    Warn,
    /// Error finding that should cause failure.
    Error,
}

/// A single finding from a dependency check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Severity of this finding.
    pub severity: Severity,
    /// Machine-readable code identifying the finding type.
    pub code: String,
    /// Human-readable description of the issue.
    pub message: String,
    /// Path to the file where the issue was found.
    pub path: Option<String>,
    /// Line number in the file (1-indexed).
    pub line: Option<u32>,
}

/// Configuration for depguard checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Check for wildcard version specifications.
    #[serde(default = "Config::default_check_wildcards")]
    pub check_wildcards: bool,

    /// Check that path dependencies also specify a version.
    #[serde(default = "Config::default_check_path_version")]
    pub check_path_version: bool,

    /// Check that members use workspace inheritance where possible.
    #[serde(default = "Config::default_check_workspace_inheritance")]
    pub check_workspace_inheritance: bool,

    /// Default severity for findings.
    #[serde(default = "Config::default_severity")]
    pub severity: Severity,

    /// Dependencies to ignore (crate names).
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl Config {
    fn default_check_wildcards() -> bool {
        true
    }
    fn default_check_path_version() -> bool {
        true
    }
    fn default_check_workspace_inheritance() -> bool {
        true
    }
    fn default_severity() -> Severity {
        Severity::Warn
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            check_wildcards: Self::default_check_wildcards(),
            check_path_version: Self::default_check_path_version(),
            check_workspace_inheritance: Self::default_check_workspace_inheritance(),
            severity: Self::default_severity(),
            ignore: Vec::new(),
        }
    }
}

/// Parsed dependency from Cargo.toml.
#[derive(Debug)]
struct Dependency {
    #[allow(dead_code)]
    name: String,
    version: Option<String>,
    path: Option<String>,
    workspace: bool,
}

/// Parsed workspace info.
#[derive(Debug, Default)]
struct WorkspaceInfo {
    /// Dependencies defined in [workspace.dependencies]
    dependencies: BTreeMap<String, WorkspaceDep>,
    /// Package fields available in [workspace.package]
    package_fields: Vec<String>,
}

/// A dependency in workspace.dependencies.
#[derive(Debug)]
struct WorkspaceDep {
    #[allow(dead_code)]
    version: Option<String>,
}

/// Run all dependency hygiene checks on a workspace.
///
/// Returns a list of findings. An empty list means all checks passed.
pub fn check_workspace(root: &Utf8Path, config: &Config) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();

    // Read root Cargo.toml
    let root_toml_path = root.join("Cargo.toml");
    let root_content = std::fs::read_to_string(&root_toml_path)
        .with_context(|| format!("read {}", root_toml_path))?;
    let root_doc: toml::Value =
        toml::from_str(&root_content).with_context(|| format!("parse {}", root_toml_path))?;

    // Parse workspace info
    let workspace_info = parse_workspace_info(&root_doc);

    // Check if this is a workspace or single crate
    let members = get_workspace_members(&root_doc, root)?;

    if members.is_empty() {
        // Single crate, check it directly
        let root_path = &root_toml_path;
        let root_doc = &root_doc;
        let workspace_info = &workspace_info;
        check_manifest(root_path, root_doc, workspace_info, config, &mut findings)?;
    } else {
        // Check each member
        for member_path in members {
            let manifest_path = member_path.join("Cargo.toml");
            if !manifest_path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("read {}", manifest_path))?;
            let doc: toml::Value =
                toml::from_str(&content).with_context(|| format!("parse {}", manifest_path))?;
            check_manifest(&manifest_path, &doc, &workspace_info, config, &mut findings)?;
        }
    }

    Ok(findings)
}

fn parse_workspace_info(doc: &toml::Value) -> WorkspaceInfo {
    let mut info = WorkspaceInfo::default();

    // Parse workspace.dependencies
    if let Some(ws_deps) = doc
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, value) in ws_deps {
            let version = match value {
                toml::Value::String(v) => Some(v.clone()),
                toml::Value::Table(t) => {
                    t.get("version").and_then(|v| v.as_str()).map(String::from)
                }
                _ => None,
            };
            info.dependencies
                .insert(name.clone(), WorkspaceDep { version });
        }
    }

    // Parse workspace.package fields
    if let Some(ws_pkg) = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.as_table())
    {
        info.package_fields = ws_pkg.keys().cloned().collect();
    }

    info
}

fn get_workspace_members(doc: &toml::Value, root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let Some(members) = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    else {
        return Ok(Vec::new());
    };

    let mut paths = Vec::new();
    for member in members {
        if let Some(pattern) = member.as_str() {
            // Handle glob patterns
            if pattern.contains('*') {
                let glob = globset::Glob::new(pattern)
                    .with_context(|| format!("invalid glob pattern: {}", pattern))?
                    .compile_matcher();

                // Walk the root directory looking for matches
                for entry in walkdir(root)? {
                    let rel = entry
                        .strip_prefix(root)
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    if glob.is_match(&rel) && entry.join("Cargo.toml").exists() {
                        paths.push(entry);
                    }
                }
            } else {
                paths.push(root.join(pattern));
            }
        }
    }

    Ok(paths)
}

fn walkdir(root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if !dir.is_dir() {
            continue;
        }
        dirs.push(dir.clone());

        for entry in std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            if let Ok(path) = Utf8PathBuf::try_from(entry.path())
                && path.is_dir()
            {
                // Skip hidden dirs and target
                let name = path.file_name().unwrap_or("");
                if !name.starts_with('.') && name != "target" {
                    stack.push(path);
                }
            }
        }
    }

    Ok(dirs)
}

fn check_manifest(
    path: &Utf8Path,
    doc: &toml::Value,
    workspace_info: &WorkspaceInfo,
    config: &Config,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    let rel_path = path.to_string();

    // Check [dependencies], [dev-dependencies], [build-dependencies]
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = doc.get(section).and_then(|d| d.as_table()) {
            for (name, value) in deps {
                if config.ignore.contains(name) {
                    continue;
                }

                let dep = parse_dependency(name, value);

                // Check for wildcard versions
                if config.check_wildcards
                    && let Some(ref v) = dep.version
                    && v == "*"
                {
                    findings.push(Finding {
                        severity: config.severity,
                        code: "wildcard_version".to_string(),
                        message: format!(
                            "{}: dependency '{}' uses wildcard version '*'",
                            section, name
                        ),
                        path: Some(rel_path.clone()),
                        line: None,
                    });
                }

                // Check path deps have version for publishing
                if config.check_path_version
                    && dep.path.is_some()
                    && dep.version.is_none()
                    && !dep.workspace
                {
                    findings.push(Finding {
                        severity: config.severity,
                        code: "path_missing_version".to_string(),
                        message: format!(
                            "{}: path dependency '{}' should specify version for publishing",
                            section, name
                        ),
                        path: Some(rel_path.clone()),
                        line: None,
                    });
                }

                // Check workspace inheritance
                if config.check_workspace_inheritance
                    && !dep.workspace
                    && workspace_info.dependencies.contains_key(name)
                {
                    findings.push(Finding {
                        severity: Severity::Info,
                        code: "missing_workspace_inheritance".to_string(),
                        message: format!(
                            "{}: dependency '{}' could use workspace inheritance (foo.workspace = true)",
                            section, name
                        ),
                        path: Some(rel_path.clone()),
                        line: None,
                    });
                }
            }
        }
    }

    Ok(())
}

fn parse_dependency(name: &str, value: &toml::Value) -> Dependency {
    match value {
        toml::Value::String(v) => Dependency {
            name: name.to_string(),
            version: Some(v.clone()),
            path: None,
            workspace: false,
        },
        toml::Value::Table(t) => Dependency {
            name: name.to_string(),
            version: t.get("version").and_then(|v| v.as_str()).map(String::from),
            path: t.get("path").and_then(|v| v.as_str()).map(String::from),
            workspace: t
                .get("workspace")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
        _ => Dependency {
            name: name.to_string(),
            version: None,
            path: None,
            workspace: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_workspace(root_toml: &str, members: &[(&str, &str)]) -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();

        std::fs::write(root.join("Cargo.toml"), root_toml).unwrap();

        for (path, content) in members {
            let member_dir = root.join(path);
            std::fs::create_dir_all(&member_dir).unwrap();
            std::fs::write(member_dir.join("Cargo.toml"), content).unwrap();
        }

        (temp, root)
    }

    #[test]
    fn test_detects_wildcard_version() {
        let (_temp, root) = create_test_workspace(
            r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "*"
"#,
            &[],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.iter().any(|f| f.code == "wildcard_version"));
    }

    #[test]
    fn test_detects_path_without_version() {
        let (_temp, root) = create_test_workspace(
            r#"
            [package]
            name = "test"
            version = "0.1.0"

            [dependencies]
            my-crate = { path = "../my-crate" }
            "#,
            &[],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.iter().any(|f| f.code == "path_missing_version"));
    }

    #[test]
    fn test_path_with_version_passes() {
        let (_temp, root) = create_test_workspace(
            r#"
            [package]
            name = "test"
            version = "0.1.0"

            [dependencies]
            my-crate = { path = "../my-crate", version = "0.1.0" }
            "#,
            &[],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(!findings.iter().any(|f| f.code == "path_missing_version"));
    }

    #[test]
    fn test_workspace_inheritance_suggestion() {
        let (_temp, root) = create_test_workspace(
            r#"
            [workspace]
            members = ["crates/foo"]

            [workspace.dependencies]
            serde = "1.0"
            "#,
            &[(
                "crates/foo",
                r#"
                [package]
                name = "foo"
                version = "0.1.0"

                [dependencies]
                serde = "1.0"
                "#,
            )],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "missing_workspace_inheritance")
        );
    }

    #[test]
    fn test_workspace_inheritance_used_passes() {
        let (_temp, root) = create_test_workspace(
            r#"
            [workspace]
            members = ["crates/foo"]

            [workspace.dependencies]
            serde = "1.0"
            "#,
            &[(
                "crates/foo",
                r#"
                [package]
                name = "foo"
                version = "0.1.0"

                [dependencies]
                serde.workspace = true
                "#,
            )],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == "missing_workspace_inheritance")
        );
    }

    #[test]
    fn test_workspace_inheritance_with_table_dependency() {
        let (_temp, root) = create_test_workspace(
            r#"
            [workspace]
            members = ["crates/foo"]

            [workspace.dependencies]
            serde = { version = "1.0" }
            "#,
            &[(
                "crates/foo",
                r#"
                [package]
                name = "foo"
                version = "0.1.0"

                [dependencies]
                serde = "1.0"
                "#,
            )],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "missing_workspace_inheritance")
        );
    }

    #[test]
    fn test_check_workspace_with_members() {
        let (_temp, root) = create_test_workspace(
            r#"
            [workspace]
            members = ["crates/foo"]
            "#,
            &[(
                "crates/foo",
                r#"
                [package]
                name = "foo"
                version = "0.1.0"

                [dependencies]
                serde = "*"
                "#,
            )],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.iter().any(|f| f.code == "wildcard_version"));
    }

    #[test]
    fn test_check_workspace_skips_missing_member_manifest() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/missing"]
            "#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/missing")).unwrap();

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_dependency_non_string_value() {
        let dep = parse_dependency("weird", &toml::Value::Integer(1));
        assert_eq!(dep.name, "weird");
        assert!(dep.version.is_none());
        assert!(dep.path.is_none());
        assert!(!dep.workspace);
    }

    #[test]
    fn test_walkdir_non_directory_root() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();
        let file_path = root.join("file.txt");
        std::fs::write(&file_path, "data").unwrap();

        let dirs = walkdir(&file_path).unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_ignore_list() {
        let (_temp, root) = create_test_workspace(
            r#"
            [package]
            name = "test"
            version = "0.1.0"

            [dependencies]
            serde = "*"
            "#,
            &[],
        );

        let config = Config {
            ignore: vec!["serde".to_string()],
            ..Default::default()
        };

        let findings = check_workspace(&root, &config).unwrap();
        assert!(!findings.iter().any(|f| f.code == "wildcard_version"));
    }

    #[test]
    fn test_workspace_members_glob_pattern() {
        let (_temp, root) = create_test_workspace(
            r#"
[workspace]
members = ["crates/*"]
            "#,
            &[
                (
                    "crates/foo",
                    r#"
[package]
name = "foo"
version = "0.1.0"

[dependencies]
serde = "*"
                    "#,
                ),
                (
                    "crates/bar",
                    r#"
[package]
name = "bar"
version = "0.1.0"

[dependencies]
anyhow = "1.0"
                    "#,
                ),
            ],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.iter().any(|f| f.code == "wildcard_version"));
    }

    #[test]
    fn test_check_workspace_single_crate_runs() {
        let (_temp, root) = create_test_workspace(
            r#"
[package]
name = "single"
version = "0.1.0"
edition = "2021"
"#,
            &[],
        );

        let findings = check_workspace(&root, &Config::default()).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn test_parse_workspace_info_handles_non_string_dependency() {
        let doc: toml::Value = toml::from_str(
            r#"
[workspace.dependencies]
serde = 1
            "#,
        )
        .unwrap();

        let info = parse_workspace_info(&doc);
        assert!(info.dependencies.contains_key("serde"));
        assert!(info.dependencies.get("serde").unwrap().version.is_none());
    }

    #[test]
    fn test_get_workspace_members_skips_non_string_entries() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();
        let doc: toml::Value = toml::from_str(
            r#"
[workspace]
members = [1]
            "#,
        )
        .unwrap();

        let members = get_workspace_members(&doc, &root).unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn test_walkdir_collects_directories() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("crates/foo")).unwrap();

        let dirs = walkdir(&root).unwrap();
        assert!(dirs.iter().any(|d| d.ends_with("crates/foo")));
    }

    #[test]
    fn test_parse_workspace_info_captures_dependencies_and_package_fields() {
        let doc: toml::Value = toml::from_str(
            r#"
[workspace.dependencies]
serde = "1.0"

[workspace.package]
edition = "2021"
            "#,
        )
        .unwrap();

        let info = parse_workspace_info(&doc);
        assert!(info.dependencies.contains_key("serde"));
        assert!(info.package_fields.contains(&"edition".to_string()));
    }

    #[test]
    fn test_walkdir_skips_hidden_and_target() {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::try_from(temp.path().to_path_buf()).unwrap();

        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join("crates/foo")).unwrap();

        let dirs = walkdir(&root).unwrap();
        let dir_strs: Vec<String> = dirs.iter().map(|d| d.to_string()).collect();
        assert!(!dir_strs.iter().any(|d| d.contains(".git")));
        assert!(!dir_strs.iter().any(|d| d.ends_with("target")));
    }
}
