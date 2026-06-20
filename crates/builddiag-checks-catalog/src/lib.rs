//! Check catalog and metadata for builddiag built-in checks.
//!
//! This crate provides a tiny, dependency-light registry for check IDs, docs,
//! default severity/trigger metadata, and explain lookup.

use builddiag_types::Severity;

/// Documentation for a check, used by the explain APIs and CLI rendering.
#[derive(Debug, Clone)]
pub struct CheckDocumentation {
    /// Check ID (for example, `rust.msrv_defined`).
    pub id: &'static str,
    /// Human-readable check name.
    pub name: &'static str,
    /// Detailed description of the check contract.
    pub description: &'static str,
    /// Short remediation guidance.
    pub help: &'static str,
    /// Optional documentation URL.
    pub url: Option<&'static str>,
    /// Finding codes produced by this check.
    pub codes: &'static [&'static str],
}

/// Static definition for a built-in check.
#[derive(Debug, Clone)]
pub struct CheckDef {
    /// Stable check identifier.
    pub id: &'static str,
    /// Default severity when enabled.
    pub default_severity: Severity,
    /// Default file patterns that trigger this check.
    pub default_triggers: &'static [&'static str],
}

/// Exposed check documentation registry.
pub static CHECK_DOCS: &[CheckDocumentation] = &[
    #[cfg(feature = "msrv")]
    CheckDocumentation {
        id: "rust.msrv_defined",
        name: "MSRV Defined",
        description: "Validates that the Minimum Supported Rust Version (MSRV) is explicitly defined in Cargo.toml. MSRV helps users and CI systems know which Rust version is required to build your crate.",
        help: "Add `rust-version = \"1.XX.0\"` to your workspace Cargo.toml under [workspace.package] or [package].",
        url: Some("https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field"),
        codes: &["missing_msrv", "invalid_msrv_defined"],
    },
    #[cfg(feature = "msrv")]
    CheckDocumentation {
        id: "rust.msrv_consistent",
        name: "MSRV Consistent",
        description: "Validates that all workspace members have consistent MSRV values. Inconsistent MSRV across crates can cause confusing build failures.",
        help: "Ensure all crates either inherit from workspace.package.rust-version or explicitly set the same rust-version.",
        url: Some("https://doc.rust-lang.org/cargo/reference/workspaces.html"),
        codes: &[
            "invalid_msrv",
            "missing_member_msrv",
            "invalid_member_msrv",
            "msrv_mismatch",
        ],
    },
    #[cfg(feature = "toolchain")]
    CheckDocumentation {
        id: "rust.toolchain_pinning",
        name: "Toolchain Pinning",
        description: "Validates that rust-toolchain.toml pins the Rust version to a specific release (e.g., \"1.75.0\") rather than a moving target like \"stable\".",
        help: "Create rust-toolchain.toml with `channel = \"1.XX.0\"` to pin the version.",
        url: Some("https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file"),
        codes: &[
            "missing_toolchain",
            "nightly_disallowed",
            "unpinned_channel",
            "invalid_toolchain_version",
        ],
    },
    #[cfg(feature = "toolchain")]
    CheckDocumentation {
        id: "rust.toolchain_msrv_relation",
        name: "Toolchain-MS RV Relation",
        description: "Validates that the pinned toolchain version matches or exceeds the MSRV. This ensures CI tests against the version users will actually use.",
        help: "Set your toolchain channel to match your MSRV, or configure policy.toolchain.relation_to_msrv = \"at_least\" to allow newer toolchains.",
        url: None,
        codes: &["toolchain_msrv_mismatch"],
    },
    #[cfg(feature = "checksums")]
    CheckDocumentation {
        id: "tools.checksums_file_exists",
        name: "Checksums File Exists",
        description: "Validates that the tools checksums file (scripts/tools.sha256) exists. This file contains SHA256 hashes for tool binaries to verify integrity.",
        help: "Create scripts/tools.sha256 with checksums in the format: `<sha256hash>  <filepath>`",
        url: None,
        codes: &["missing_checksums"],
    },
    #[cfg(feature = "checksums")]
    CheckDocumentation {
        id: "tools.checksums_format",
        name: "Checksums Format",
        description: "Validates that the checksums file has valid format: 64-character hex SHA256 hashes followed by file paths, no duplicates.",
        help: "Ensure each line follows the format: `<64-char-sha256>  <filepath>`. Generate with: `sha256sum <file>`",
        url: None,
        codes: &["invalid_hash", "missing_path", "duplicate_path"],
    },
    #[cfg(feature = "checksums")]
    CheckDocumentation {
        id: "tools.checksums_coverage",
        name: "Checksums Coverage",
        description: "Validates that all tool files listed in the tools manifest have corresponding checksum entries.",
        help: "Add missing checksums for all files listed in scripts/tools.toml.",
        url: None,
        codes: &["missing_checksum", "unexpected_checksum"],
    },
    #[cfg(feature = "checksums")]
    CheckDocumentation {
        id: "tools.checksums_verify_local",
        name: "Checksums Verify Local",
        description: "Verifies that local tool files match their recorded checksums. Detects tampering or corruption of tool binaries.",
        help: "Re-download or regenerate tools with mismatched checksums, then update scripts/tools.sha256.",
        url: None,
        codes: &["missing_tool_file", "hash_mismatch"],
    },
    #[cfg(feature = "workspace")]
    CheckDocumentation {
        id: "workspace.resolver_v2",
        name: "Workspace Resolver v2",
        description: "Validates that Cargo workspaces use resolver version 2. Resolver v2 has better feature unification and is required for edition 2021+.",
        help: "Add `resolver = \"2\"` to your [workspace] section in Cargo.toml.",
        url: Some(
            "https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2",
        ),
        codes: &["resolver_not_v2"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.wildcard_version",
        name: "No Wildcard Versions",
        description: "Validates that dependencies do not use wildcard version specifications (\"*\"). Wildcard versions are fragile and can cause unexpected breakage.",
        help: "Replace `foo = \"*\"` with a specific version like `foo = \"1.0\"`.",
        url: None,
        codes: &["wildcard_version"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.path_missing_version",
        name: "Path Dependencies Have Version",
        description: "Validates that path dependencies also specify a version. Path-only dependencies cannot be published to crates.io.",
        help: "Add a version field: `foo = { path = \"../foo\", version = \"0.1\" }`.",
        url: None,
        codes: &["path_missing_version"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.workspace_inheritance",
        name: "Workspace Inheritance",
        description: "Suggests using workspace dependency inheritance when a dependency is defined in workspace.dependencies.",
        help: "Use `foo.workspace = true` instead of duplicating the version.",
        url: None,
        codes: &["missing_workspace_inheritance"],
    },
    #[cfg(feature = "workspace")]
    CheckDocumentation {
        id: "workspace.edition_consistent",
        name: "Edition Consistent",
        description: "Validates that all workspace members use the same Rust edition. Inconsistent editions across crates can cause confusing behavior differences.",
        help: "Ensure all crates either inherit from workspace.package.edition or explicitly set the same edition.",
        url: Some("https://doc.rust-lang.org/edition-guide/"),
        codes: &[
            "invalid_workspace_edition",
            "missing_member_edition",
            "invalid_member_edition",
            "edition_mismatch",
        ],
    },
    #[cfg(feature = "workspace")]
    CheckDocumentation {
        id: "workspace.member_ordering",
        name: "Member Ordering",
        description: "Validates that workspace members in [workspace.members] are sorted alphabetically. Sorted members improve readability and reduce merge conflicts.",
        help: "Sort the members array alphabetically in Cargo.toml.",
        url: None,
        codes: &["members_not_sorted"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.lockfile_present",
        name: "Lockfile Present",
        description: "Validates that Cargo.lock exists for binary crates. A lockfile ensures reproducible builds for applications.",
        help: "Run `cargo build` to generate Cargo.lock and commit it to version control.",
        url: Some(
            "https://doc.rust-lang.org/cargo/faq.html#why-do-binaries-have-cargolock-in-version-control-but-not-libraries",
        ),
        codes: &[
            "missing_lockfile_for_binary",
            "unexpected_lockfile_for_library",
        ],
    },
    #[cfg(feature = "publish")]
    CheckDocumentation {
        id: "workspace.publish_ready",
        name: "Publish Ready",
        description: "Validates that publishable crates have required metadata for crates.io. Required fields include description and license (or license-file). Recommended fields include repository, documentation, and keywords.",
        help: "Add the missing metadata fields to your Cargo.toml [package] section.",
        url: Some("https://doc.rust-lang.org/cargo/reference/manifest.html#the-package-section"),
        codes: &[
            "missing_description",
            "missing_license",
            "missing_repository",
            "missing_documentation",
            "missing_readme",
        ],
    },
    #[cfg(feature = "workspace")]
    CheckDocumentation {
        id: "rust.edition_deprecations",
        name: "Edition Deprecations",
        description: "Warns about deprecated edition features and migration opportunities. Older editions may have deprecated syntax or missing modern features.",
        help: "Consider migrating to a newer Rust edition using `cargo fix --edition`.",
        url: Some("https://doc.rust-lang.org/edition-guide/"),
        codes: &["deprecated_edition", "edition_migration_available"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.duplicate_versions",
        name: "Duplicate Dependency Versions",
        description: "Detects when the same dependency is specified with different versions across workspace members. This can lead to larger binaries and potential compatibility issues.",
        help: "Unify dependency versions using [workspace.dependencies] inheritance.",
        url: Some(
            "https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table",
        ),
        codes: &["duplicate_dependency_version"],
    },
    #[cfg(feature = "deps")]
    CheckDocumentation {
        id: "deps.security_advisory",
        name: "Security Advisory",
        description: "Checks dependencies against the RustSec advisory database for known security vulnerabilities. Requires the 'security' feature to be enabled.",
        help: "Update affected dependencies to patched versions or review advisories for mitigations.",
        url: Some("https://rustsec.org/"),
        codes: &[
            "security_vulnerability",
            "security_unmaintained",
            "security_yanked",
        ],
    },
];

/// Built-in check metadata.
pub static BUILTIN_CHECKS: &[CheckDef] = &[
    #[cfg(feature = "msrv")]
    CheckDef {
        id: "rust.msrv_defined",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "msrv")]
    CheckDef {
        id: "rust.msrv_consistent",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "toolchain")]
    CheckDef {
        id: "rust.toolchain_pinning",
        default_severity: Severity::Error,
        default_triggers: &["rust-toolchain", "rust-toolchain.toml"],
    },
    #[cfg(feature = "toolchain")]
    CheckDef {
        id: "rust.toolchain_msrv_relation",
        default_severity: Severity::Error,
        default_triggers: &[
            "rust-toolchain",
            "rust-toolchain.toml",
            "Cargo.toml",
            "**/Cargo.toml",
        ],
    },
    #[cfg(feature = "checksums")]
    CheckDef {
        id: "tools.checksums_file_exists",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    #[cfg(feature = "checksums")]
    CheckDef {
        id: "tools.checksums_format",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256"],
    },
    #[cfg(feature = "checksums")]
    CheckDef {
        id: "tools.checksums_coverage",
        default_severity: Severity::Error,
        default_triggers: &["scripts/tools.sha256", "scripts/tools.toml"],
    },
    #[cfg(feature = "checksums")]
    CheckDef {
        id: "tools.checksums_verify_local",
        default_severity: Severity::Warn,
        default_triggers: &["scripts/tools.sha256"],
    },
    #[cfg(feature = "workspace")]
    CheckDef {
        id: "workspace.resolver_v2",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.wildcard_version",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.path_missing_version",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.workspace_inheritance",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "workspace")]
    CheckDef {
        id: "workspace.edition_consistent",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "workspace")]
    CheckDef {
        id: "workspace.member_ordering",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.lockfile_present",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.lock", "Cargo.toml"],
    },
    #[cfg(feature = "publish")]
    CheckDef {
        id: "workspace.publish_ready",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "workspace")]
    CheckDef {
        id: "rust.edition_deprecations",
        default_severity: Severity::Info,
        default_triggers: &["Cargo.toml", "**/Cargo.toml"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.duplicate_versions",
        default_severity: Severity::Warn,
        default_triggers: &["Cargo.toml", "**/Cargo.toml", "Cargo.lock"],
    },
    #[cfg(feature = "deps")]
    CheckDef {
        id: "deps.security_advisory",
        default_severity: Severity::Error,
        default_triggers: &["Cargo.lock", "Cargo.toml"],
    },
];

/// Lookup documentation by check ID or finding code.
pub fn explain_check(check_or_code: &str) -> Option<&'static CheckDocumentation> {
    if let Some(doc) = CHECK_DOCS.iter().find(|d| d.id == check_or_code) {
        return Some(doc);
    }

    CHECK_DOCS.iter().find(|d| d.codes.contains(&check_or_code))
}
