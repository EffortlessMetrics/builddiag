//! Property-based tests for crate metadata completeness.
//!
//! Feature: release-ready, Property 1: Crate Metadata Completeness
//!
//! **Validates: Requirements 3.1, 3.2**

use proptest::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Required metadata fields for all crates
const REQUIRED_FIELDS: &[&str] = &[
    "description",
    "repository",
    "homepage",
    "keywords",
    "categories",
];

/// All workspace crates that must have complete metadata
const WORKSPACE_CRATES: &[&str] = &[
    "builddiag-types",
    "builddiag-domain",
    "builddiag-repo",
    "builddiag-checks",
    "builddiag-render",
    "builddiag-app",
    "builddiag", // CLI crate (in builddiag-cli directory)
];

/// Maps crate names to their directory names
fn crate_dir(crate_name: &str) -> &str {
    match crate_name {
        "builddiag" => "builddiag-cli",
        other => other,
    }
}

/// Parses a Cargo.toml file and extracts the [package] section
fn parse_cargo_toml(path: &Path) -> BTreeMap<String, toml::Value> {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    let parsed: toml::Value = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e));

    parsed
        .get("package")
        .and_then(|p| p.as_table())
        .cloned()
        .map(|t| t.into_iter().collect())
        .unwrap_or_default()
}

/// Checks if a metadata field is present and non-empty.
/// Supports both direct values and workspace inheritance (e.g., `field.workspace = true`).
fn has_field(package: &BTreeMap<String, toml::Value>, field: &str) -> bool {
    match package.get(field) {
        Some(toml::Value::String(s)) => !s.is_empty(),
        Some(toml::Value::Array(arr)) => !arr.is_empty(),
        Some(toml::Value::Table(t)) => {
            // Handle workspace inheritance pattern: field.workspace = true
            t.get("workspace")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// Feature: release-ready, Property 1: Crate Metadata Completeness
///
/// For any crate in the workspace, its Cargo.toml SHALL contain the required
/// metadata fields: description, repository, homepage, keywords, and categories.
///
/// **Validates: Requirements 3.1, 3.2**
#[test]
fn property_all_crates_have_required_metadata() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    for crate_name in WORKSPACE_CRATES {
        let dir = crate_dir(crate_name);
        let cargo_toml_path = workspace_root.join("crates").join(dir).join("Cargo.toml");

        assert!(
            cargo_toml_path.exists(),
            "Cargo.toml not found for crate '{}' at {}",
            crate_name,
            cargo_toml_path.display()
        );

        let package = parse_cargo_toml(&cargo_toml_path);

        for field in REQUIRED_FIELDS {
            assert!(
                has_field(&package, field),
                "Crate '{}' is missing required metadata field '{}' in {}",
                crate_name,
                field,
                cargo_toml_path.display()
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-ready, Property 1: Crate Metadata Completeness
    ///
    /// Property test that verifies for any randomly selected crate from the workspace,
    /// all required metadata fields are present.
    ///
    /// **Validates: Requirements 3.1, 3.2**
    #[test]
    fn property_random_crate_has_complete_metadata(crate_idx in 0..WORKSPACE_CRATES.len()) {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let crate_name = WORKSPACE_CRATES[crate_idx];
        let dir = crate_dir(crate_name);
        let cargo_toml_path = workspace_root.join("crates").join(dir).join("Cargo.toml");

        prop_assert!(
            cargo_toml_path.exists(),
            "Cargo.toml not found for crate '{}' at {}",
            crate_name,
            cargo_toml_path.display()
        );

        let package = parse_cargo_toml(&cargo_toml_path);

        for field in REQUIRED_FIELDS {
            prop_assert!(
                has_field(&package, field),
                "Crate '{}' is missing required metadata field '{}'",
                crate_name,
                field
            );
        }
    }

    /// Feature: release-ready, Property 1: Crate Metadata Completeness
    ///
    /// Property test that verifies for any randomly selected required field,
    /// all workspace crates have that field present.
    ///
    /// **Validates: Requirements 3.1, 3.2**
    #[test]
    fn property_random_field_present_in_all_crates(field_idx in 0..REQUIRED_FIELDS.len()) {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let field = REQUIRED_FIELDS[field_idx];

        for crate_name in WORKSPACE_CRATES {
            let dir = crate_dir(crate_name);
            let cargo_toml_path = workspace_root.join("crates").join(dir).join("Cargo.toml");
            let package = parse_cargo_toml(&cargo_toml_path);

            prop_assert!(
                has_field(&package, field),
                "Crate '{}' is missing required metadata field '{}'",
                crate_name,
                field
            );
        }
    }
}

#[cfg(test)]
mod cli_specific_tests {
    use super::*;

    /// Verifies CLI crate has the specific keywords required by Requirement 3.3
    #[test]
    fn cli_crate_has_required_keywords() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cargo_toml_path = workspace_root
            .join("crates")
            .join("builddiag-cli")
            .join("Cargo.toml");
        let package = parse_cargo_toml(&cargo_toml_path);

        let keywords = package
            .get("keywords")
            .and_then(|v| v.as_array())
            .expect("CLI crate should have keywords array");

        let keyword_strings: Vec<&str> = keywords.iter().filter_map(|v| v.as_str()).collect();

        let required_keywords = ["rust", "cli", "build", "validation", "msrv"];
        for kw in required_keywords {
            assert!(
                keyword_strings.contains(&kw),
                "CLI crate is missing required keyword '{}'",
                kw
            );
        }
    }

    /// Verifies CLI crate has the specific categories required by Requirement 3.4
    #[test]
    fn cli_crate_has_required_categories() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cargo_toml_path = workspace_root
            .join("crates")
            .join("builddiag-cli")
            .join("Cargo.toml");
        let package = parse_cargo_toml(&cargo_toml_path);

        let categories = package
            .get("categories")
            .and_then(|v| v.as_array())
            .expect("CLI crate should have categories array");

        let category_strings: Vec<&str> = categories.iter().filter_map(|v| v.as_str()).collect();

        let required_categories = ["development-tools", "command-line-utilities"];
        for cat in required_categories {
            assert!(
                category_strings.contains(&cat),
                "CLI crate is missing required category '{}'",
                cat
            );
        }
    }
}
