//! Explain registry for builddiag checks and finding codes.
//!
//! This module provides detailed explanations for each check ID and finding code,
//! including what it means, why it matters, how to fix it, and optional links to
//! documentation.
//!
//! # Usage
//!
//! ```
//! use builddiag_domain::explain::{explain, ExplainEntry};
//!
//! // Look up by check ID
//! if let Some(entry) = explain("rust.msrv_defined") {
//!     println!("What it means: {}", entry.what_it_means);
//!     println!("Why it matters: {}", entry.why_it_matters);
//!     println!("How to fix: {}", entry.how_to_fix);
//! }
//!
//! // Look up by finding code
//! if let Some(entry) = explain("missing_msrv") {
//!     println!("Check: {}", entry.check_id);
//! }
//! ```

/// A single explanation entry for a check/code pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainEntry {
    /// Check ID (e.g., "rust.msrv_defined").
    pub check_id: &'static str,
    /// Finding code (e.g., "missing_msrv").
    pub code: &'static str,
    /// Human-readable name for this finding.
    pub name: &'static str,
    /// What this finding means - describes what the check detected.
    pub what_it_means: &'static str,
    /// Why this matters - explains the impact of not addressing it.
    pub why_it_matters: &'static str,
    /// How to fix - actionable steps to resolve the finding.
    pub how_to_fix: &'static str,
    /// Optional documentation links.
    pub links: &'static [&'static str],
}

/// Registry of all explain entries for checks and codes.
///
/// Each entry maps a (check_id, code) pair to its full explanation.
pub static EXPLAIN_REGISTRY: &[ExplainEntry] = &[
    // ==========================================================================
    // rust.msrv_defined
    // ==========================================================================
    ExplainEntry {
        check_id: "rust.msrv_defined",
        code: "missing_msrv",
        name: "Missing MSRV",
        what_it_means: "The Minimum Supported Rust Version (MSRV) is not defined in your \
                        Cargo.toml. Without an explicit MSRV, users and CI systems cannot \
                        determine which Rust version is required to build your crate.",
        why_it_matters: "Defining MSRV provides a clear contract with users about which Rust \
                         versions are supported. Without it, users may encounter confusing \
                         build failures when using older Rust versions. CI systems like \
                         rust-version-action rely on this field to test against the correct \
                         version.",
        how_to_fix: "Add `rust-version = \"1.XX.0\"` to your workspace Cargo.toml under \
                     [workspace.package] or [package]. Choose the oldest Rust version you \
                     want to support and test against it in CI.",
        links: &[
            "https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field",
            "https://rust-lang.github.io/rfcs/2495-min-rust-version.html",
        ],
    },
    // ==========================================================================
    // rust.msrv_consistent
    // ==========================================================================
    ExplainEntry {
        check_id: "rust.msrv_consistent",
        code: "invalid_msrv",
        name: "Invalid Workspace MSRV",
        what_it_means: "The workspace-level rust-version field contains an invalid version \
                        string that cannot be parsed as a valid Rust version.",
        why_it_matters: "An invalid MSRV string will cause Cargo to reject the manifest or \
                         may lead to unexpected behavior. Tools that rely on the MSRV field \
                         will not work correctly.",
        how_to_fix: "Use a valid Rust version format: either two components (e.g., \"1.75\") \
                     or three components (e.g., \"1.75.0\"). Do not include channel names \
                     like \"stable\" or prerelease suffixes.",
        links: &["https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field"],
    },
    ExplainEntry {
        check_id: "rust.msrv_consistent",
        code: "missing_member_msrv",
        name: "Missing Member MSRV",
        what_it_means: "A workspace member crate does not have a rust-version defined and \
                        is not inheriting from the workspace. This means the crate has no \
                        MSRV guarantee.",
        why_it_matters: "Inconsistent MSRV definitions across workspace members can cause \
                         confusing build failures. When some crates have MSRV and others \
                         don't, users cannot reliably determine which Rust version to use.",
        how_to_fix: "Either add `rust-version.workspace = true` to inherit from the workspace, \
                     or explicitly set `rust-version = \"X.Y.Z\"` in the member's Cargo.toml. \
                     Prefer workspace inheritance for consistency.",
        links: &["https://doc.rust-lang.org/cargo/reference/workspaces.html#the-package-table"],
    },
    ExplainEntry {
        check_id: "rust.msrv_consistent",
        code: "invalid_member_msrv",
        name: "Invalid Member MSRV",
        what_it_means: "A workspace member has a rust-version field that cannot be parsed \
                        as a valid Rust version string.",
        why_it_matters: "Invalid version strings will cause Cargo to reject the manifest \
                         or lead to unexpected behavior.",
        how_to_fix: "Correct the rust-version format in the member's Cargo.toml. Use a \
                     valid version like \"1.75\" or \"1.75.0\".",
        links: &[],
    },
    ExplainEntry {
        check_id: "rust.msrv_consistent",
        code: "msrv_mismatch",
        name: "MSRV Mismatch",
        what_it_means: "A workspace member has a different MSRV than the workspace root. \
                        This means different crates in the same workspace require different \
                        minimum Rust versions.",
        why_it_matters: "Mismatched MSRVs can cause user confusion and build failures. When \
                         crates in a workspace have different MSRVs, users may successfully \
                         build some crates but fail on others with the same Rust version.",
        how_to_fix: "Align all workspace members to use the same MSRV by using workspace \
                     inheritance (`rust-version.workspace = true`). If a specific crate \
                     genuinely needs a different MSRV, add it to the allow_overrides list \
                     in your builddiag config.",
        links: &["https://doc.rust-lang.org/cargo/reference/workspaces.html"],
    },
    // ==========================================================================
    // rust.toolchain_pinning
    // ==========================================================================
    ExplainEntry {
        check_id: "rust.toolchain_pinning",
        code: "missing_toolchain",
        name: "Missing Toolchain File",
        what_it_means: "No rust-toolchain.toml (or rust-toolchain) file was found in the \
                        repository root. Without this file, contributors will use whatever \
                        Rust version happens to be installed on their system.",
        why_it_matters: "A toolchain file ensures all contributors and CI use the same Rust \
                         version, leading to reproducible builds. Without it, builds may \
                         succeed on one machine but fail on another due to version differences.",
        how_to_fix: "Create a rust-toolchain.toml file in your repository root with content \
                     like:\n\n[toolchain]\nchannel = \"1.75.0\"\n\nPin to your MSRV or a \
                     specific recent stable version.",
        links: &["https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file"],
    },
    ExplainEntry {
        check_id: "rust.toolchain_pinning",
        code: "nightly_disallowed",
        name: "Nightly Toolchain Disallowed",
        what_it_means: "The toolchain file specifies 'nightly' as the channel, but the \
                        policy disallows nightly toolchains.",
        why_it_matters: "Nightly Rust changes daily and may break your build at any time. \
                         For production code, using a stable, pinned version ensures \
                         predictable builds. Nightly features may also be removed or changed.",
        how_to_fix: "Change the channel in rust-toolchain.toml from 'nightly' to a specific \
                     stable version (e.g., \"1.75.0\"). If you require nightly features, \
                     set policy.toolchain.allow_nightly = true in your builddiag config.",
        links: &["https://doc.rust-lang.org/book/appendix-07-nightly-rust.html"],
    },
    ExplainEntry {
        check_id: "rust.toolchain_pinning",
        code: "unpinned_channel",
        name: "Unpinned Toolchain Channel",
        what_it_means: "The toolchain channel is set to a moving target like 'stable', \
                        'beta', or 'nightly' instead of a specific version number.",
        why_it_matters: "Moving channel names like 'stable' point to different Rust versions \
                         over time. This means builds are not reproducible - code that builds \
                         today may fail tomorrow when a new Rust version is released.",
        how_to_fix: "Pin to a specific version in rust-toolchain.toml. Replace 'stable' with \
                     the actual version number (e.g., \"1.75.0\"). You can find the current \
                     stable version with `rustc --version`.",
        links: &["https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file"],
    },
    ExplainEntry {
        check_id: "rust.toolchain_pinning",
        code: "invalid_toolchain_version",
        name: "Invalid Toolchain Version",
        what_it_means: "The toolchain channel is not a valid Rust version number. It may \
                        contain typos or an unsupported format.",
        why_it_matters: "An invalid version string may cause rustup to fail or fall back \
                         to unexpected behavior.",
        how_to_fix: "Use a valid Rust version format: either two components (e.g., \"1.75\") \
                     or three components (e.g., \"1.75.0\").",
        links: &[],
    },
    // ==========================================================================
    // rust.toolchain_msrv_relation
    // ==========================================================================
    ExplainEntry {
        check_id: "rust.toolchain_msrv_relation",
        code: "toolchain_msrv_mismatch",
        name: "Toolchain/MSRV Mismatch",
        what_it_means: "The pinned toolchain version does not match the required relationship \
                        with the MSRV. By default, the toolchain should equal the MSRV.",
        why_it_matters: "When the toolchain differs from the MSRV, you may use features or \
                         syntax not available at the MSRV. This causes builds to pass in CI \
                         but fail for users on the MSRV. Testing at MSRV catches these issues.",
        how_to_fix: "Align your toolchain channel with your MSRV. If you intentionally use \
                     a newer toolchain, set policy.toolchain.relation_to_msrv = \"at_least\" \
                     in your builddiag config and ensure you have separate MSRV CI tests.",
        links: &[],
    },
    // ==========================================================================
    // workspace.resolver_v2
    // ==========================================================================
    ExplainEntry {
        check_id: "workspace.resolver_v2",
        code: "resolver_not_v2",
        name: "Resolver Not Set to v2",
        what_it_means: "The workspace does not have `resolver = \"2\"` set in Cargo.toml, \
                        or is using an older resolver version. Resolver v2 has been \
                        available since Rust 1.51 and is the default for edition 2021+.",
        why_it_matters: "Without resolver v2, the resolver version depends on the edition \
                         and may have known issues with feature unification that can cause \
                         unexpected feature activation across dependencies. Resolver v2 \
                         provides more predictable and correct behavior.",
        how_to_fix: "Add `resolver = \"2\"` to the [workspace] section in your root \
                     Cargo.toml:\n\n[workspace]\nresolver = \"2\"\nmembers = [...]",
        links: &[
            "https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2",
            "https://doc.rust-lang.org/edition-guide/rust-2021/default-cargo-resolver.html",
        ],
    },
    ExplainEntry {
        check_id: "tools.checksums_format",
        code: "missing_path",
        name: "Missing Path in Checksum",
        what_it_means: "A checksum line has a hash but no file path. Each line must have \
                        both a hash and a path.",
        why_it_matters: "Without a path, the checksum cannot be associated with any file \
                         and serves no purpose.",
        how_to_fix: "Fix the malformed line in the checksums file. The correct format is: \
                     `<64-char-sha256>  <filepath>` with two spaces between hash and path.",
        links: &[],
    },
    ExplainEntry {
        check_id: "tools.checksums_format",
        code: "duplicate_path",
        name: "Duplicate Path in Checksums",
        what_it_means: "The same file path appears multiple times in the checksums file. \
                        Each file should have exactly one checksum entry.",
        why_it_matters: "Duplicate entries cause ambiguity about which checksum is correct \
                         and may indicate accidental duplication or merge conflicts.",
        how_to_fix: "Remove duplicate entries, keeping only the correct checksum for each \
                     file path.",
        links: &[],
    },
    // ==========================================================================
    // tools.checksums_coverage
    // ==========================================================================
    ExplainEntry {
        check_id: "tools.checksums_coverage",
        code: "missing_checksum",
        name: "Missing Checksum for Tool",
        what_it_means: "A tool file listed in the tools manifest does not have a \
                        corresponding checksum entry.",
        why_it_matters: "Without a checksum, you cannot verify the integrity of this tool \
                         binary. It may have been tampered with or corrupted.",
        how_to_fix: "Generate and add the checksum for the missing file: \
                     `sha256sum <tool-file> >> scripts/tools.sha256`",
        links: &[],
    },
    ExplainEntry {
        check_id: "tools.checksums_coverage",
        code: "unexpected_checksum",
        name: "Unexpected Checksum Entry",
        what_it_means: "The checksums file contains an entry for a file that is not listed \
                        in the tools manifest.",
        why_it_matters: "Extra entries may indicate stale checksums for removed tools, or \
                         tools that were added outside the normal process.",
        how_to_fix: "Either add the file to your tools manifest if it should be tracked, \
                     or remove the checksum entry if the tool is no longer used.",
        links: &[],
    },
    // ==========================================================================
    // tools.checksums_verify_local
    // ==========================================================================
    ExplainEntry {
        check_id: "tools.checksums_verify_local",
        code: "missing_tool_file",
        name: "Tool File Not Found",
        what_it_means: "A file listed in the checksums does not exist on disk. The \
                        checksum could not be verified.",
        why_it_matters: "Missing tool files may indicate incomplete setup, accidental \
                         deletion, or .gitignore issues preventing the file from being \
                         tracked.",
        how_to_fix: "Either download/install the missing tool file, or remove its entry \
                     from the checksums file if it is no longer needed.",
        links: &[],
    },
    ExplainEntry {
        check_id: "tools.checksums_verify_local",
        code: "hash_mismatch",
        name: "Hash Mismatch",
        what_it_means: "The actual SHA256 hash of a tool file does not match the expected \
                        hash in the checksums file.",
        why_it_matters: "A hash mismatch may indicate file corruption, tampering, or that \
                         the file was updated without updating its checksum. This is a \
                         potential security concern.",
        how_to_fix: "If the file was intentionally updated, regenerate its checksum with \
                     `sha256sum <file>` and update the checksums file. If unexpected, \
                     investigate the source of the change and consider re-downloading \
                     from a trusted source.",
        links: &[],
    },
    // ==========================================================================
    // tools.checksums_file_exists
    // ==========================================================================
    ExplainEntry {
        check_id: "tools.checksums_file_exists",
        code: "missing_checksums",
        name: "Missing Checksums File",
        what_it_means: "No checksums file was found in the repository. The checksums file \
                        (scripts/tools.sha256) declares SHA256 hashes for external tools.",
        why_it_matters: "A checksums file enables verification of tool integrity for supply \
                         chain security. Without it, tool binaries cannot be verified \
                         against known-good hashes.",
        how_to_fix: "Create scripts/tools.sha256 with SHA256 checksums for each tool. \
                     Format: `<64-char-sha256>  <filepath>`",
        links: &[],
    },
    // ==========================================================================
    // tools.checksums_format
    // ==========================================================================
    ExplainEntry {
        check_id: "tools.checksums_format",
        code: "invalid_hash",
        name: "Invalid Checksum Hash",
        what_it_means: "A checksum entry contains an invalid SHA256 hash. The hash must \
                        be exactly 64 hexadecimal characters.",
        why_it_matters: "Invalid hashes cannot be used for verification and may indicate \
                         corruption or manual editing errors in the checksums file.",
        how_to_fix: "Ensure each hash is exactly 64 hexadecimal characters (0-9, a-f). \
                     Generate hashes using: `sha256sum <file>`",
        links: &[],
    },
    // ==========================================================================
    // tools.manifest_coverage
    // ==========================================================================
    ExplainEntry {
        check_id: "tools.manifest_coverage",
        code: "incomplete",
        name: "Incomplete Manifest Coverage",
        what_it_means: "Some expected tools are not listed in the manifest. The expected \
                        tools list is configurable.",
        why_it_matters: "Unlisted tools cannot be verified and may represent untracked \
                         dependencies or security risks.",
        how_to_fix: "Add the missing tools to your manifest file. Ensure all tools \
                     used by your project are properly documented and checksummed.",
        links: &[],
    },
    // ==========================================================================
    // workspace.publish_ready
    // ==========================================================================
    ExplainEntry {
        check_id: "workspace.publish_ready",
        code: "missing_description",
        name: "Missing Description",
        what_it_means: "The crate is missing a description field which is required for \
                        publishing to crates.io.",
        why_it_matters: "crates.io requires a description field. Without it, users cannot \
                         understand what your crate does from the registry listing.",
        how_to_fix: "Add `description = \"A brief description of your crate\"` to your \
                     Cargo.toml [package] section.",
        links: &["https://doc.rust-lang.org/cargo/reference/manifest.html#the-description-field"],
    },
    ExplainEntry {
        check_id: "workspace.publish_ready",
        code: "missing_license",
        name: "Missing License",
        what_it_means: "The crate is missing both license and license-file fields, which \
                        is required for publishing to crates.io.",
        why_it_matters: "crates.io requires license information. Without it, users cannot \
                         determine if they can legally use your crate.",
        how_to_fix: "Add `license = \"MIT OR Apache-2.0\"` (or your chosen license) to your \
                     Cargo.toml [package] section. Use SPDX identifiers.",
        links: &[
            "https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields",
        ],
    },
    ExplainEntry {
        check_id: "workspace.publish_ready",
        code: "missing_repository",
        name: "Missing Repository",
        what_it_means: "The crate is missing a repository field. While not required for \
                        publishing, it is recommended.",
        why_it_matters: "The repository field helps users find your source code, report \
                         issues, and contribute. It improves discoverability and trust.",
        how_to_fix: "Add `repository = \"https://github.com/user/repo\"` to your \
                     Cargo.toml [package] section.",
        links: &["https://doc.rust-lang.org/cargo/reference/manifest.html#the-repository-field"],
    },
    ExplainEntry {
        check_id: "workspace.publish_ready",
        code: "missing_documentation",
        name: "Missing Documentation Link",
        what_it_means: "The crate is missing both documentation and homepage fields. \
                        While not required, at least one is recommended.",
        why_it_matters: "Documentation links help users find API docs and learn how to \
                         use your crate. docs.rs is automatically generated but a custom \
                         link can point to tutorials or guides.",
        how_to_fix: "Add `documentation = \"https://docs.rs/your-crate\"` or \
                     `homepage = \"https://your-project.dev\"` to your Cargo.toml.",
        links: &["https://doc.rust-lang.org/cargo/reference/manifest.html#the-documentation-field"],
    },
    ExplainEntry {
        check_id: "workspace.publish_ready",
        code: "missing_readme",
        name: "Missing Readme",
        what_it_means: "The crate is missing a readme field. While not required, it is \
                        recommended for better crates.io presentation.",
        why_it_matters: "The readme is displayed on crates.io and helps users understand \
                         your crate at a glance. It typically includes usage examples.",
        how_to_fix: "Add `readme = \"README.md\"` to your Cargo.toml [package] section \
                     and ensure the file exists.",
        links: &["https://doc.rust-lang.org/cargo/reference/manifest.html#the-readme-field"],
    },
    // ==========================================================================
    // rust.edition_deprecations
    // ==========================================================================
    ExplainEntry {
        check_id: "rust.edition_deprecations",
        code: "deprecated_edition",
        name: "Deprecated Edition",
        what_it_means: "The crate is using an outdated Rust edition that may have deprecated \
                        features or syntax.",
        why_it_matters: "Older editions miss out on ergonomic improvements and may have \
                         deprecated patterns that will eventually require migration anyway. \
                         Edition 2015 in particular lacks many modern conveniences.",
        how_to_fix: "Update your edition field and run `cargo fix --edition` to migrate. \
                     Consider moving to edition 2021 or later for the best experience.",
        links: &["https://doc.rust-lang.org/edition-guide/"],
    },
    ExplainEntry {
        check_id: "rust.edition_deprecations",
        code: "edition_migration_available",
        name: "Edition Migration Available",
        what_it_means: "A newer Rust edition is available that your crate could migrate to.",
        why_it_matters: "Newer editions include ergonomic improvements, new features, and \
                         better defaults. Staying current reduces future migration burden.",
        how_to_fix: "Run `cargo fix --edition` to automatically migrate, then update the \
                     edition field in Cargo.toml.",
        links: &[
            "https://doc.rust-lang.org/edition-guide/editions/transitioning-an-existing-project-to-a-new-edition.html",
        ],
    },
    // ==========================================================================
    // deps.duplicate_versions
    // ==========================================================================
    ExplainEntry {
        check_id: "deps.duplicate_versions",
        code: "duplicate_dependency_version",
        name: "Duplicate Dependency Versions",
        what_it_means: "The same dependency is specified with different versions across \
                        workspace members.",
        why_it_matters: "Multiple versions of the same dependency increase binary size, \
                         compile time, and can cause subtle bugs if types from different \
                         versions are accidentally mixed.",
        how_to_fix: "Unify versions by defining the dependency in [workspace.dependencies] \
                     and using `dep.workspace = true` in member crates. This ensures all \
                     crates use the same version.",
        links: &[
            "https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table",
        ],
    },
    // ==========================================================================
    // deps.security_advisory
    // ==========================================================================
    ExplainEntry {
        check_id: "deps.security_advisory",
        code: "security_vulnerability",
        name: "Security Vulnerability",
        what_it_means: "A dependency has a known security vulnerability listed in the \
                        RustSec advisory database.",
        why_it_matters: "Security vulnerabilities can expose your application to attacks. \
                         Even if your code doesn't trigger the vulnerability, it's best \
                         practice to update to patched versions.",
        how_to_fix: "Update the affected dependency to a patched version. Check the \
                     advisory for specific version requirements and any workarounds.",
        links: &["https://rustsec.org/"],
    },
    ExplainEntry {
        check_id: "deps.security_advisory",
        code: "security_unmaintained",
        name: "Unmaintained Dependency",
        what_it_means: "A dependency is marked as unmaintained in the RustSec database.",
        why_it_matters: "Unmaintained crates won't receive security fixes or updates. \
                         Consider finding an alternative or forking if necessary.",
        how_to_fix: "Look for maintained alternatives or consider if the dependency is \
                     still necessary. The advisory may suggest replacements.",
        links: &["https://rustsec.org/"],
    },
    ExplainEntry {
        check_id: "deps.security_advisory",
        code: "security_yanked",
        name: "Yanked Dependency",
        what_it_means: "A dependency version has been yanked from crates.io.",
        why_it_matters: "Yanked versions typically have bugs or security issues. While \
                         existing Cargo.lock files continue to work, new builds may fail.",
        how_to_fix: "Update to a non-yanked version of the dependency.",
        links: &["https://doc.rust-lang.org/cargo/commands/cargo-yank.html"],
    },
];

/// Look up an explanation by check ID or finding code.
///
/// This function searches the explain registry for a matching entry. It first
/// tries to match on check_id, then on code if no check match is found.
///
/// # Arguments
///
/// * `check_or_code` - Either a check ID (e.g., "rust.msrv_defined") or a
///   finding code (e.g., "missing_msrv")
///
/// # Returns
///
/// - `Some(entry)` if a matching entry is found
/// - `None` if no match exists
///
/// # Examples
///
/// ```
/// use builddiag_domain::explain::explain;
///
/// // Look up by check ID
/// let entry = explain("rust.msrv_defined").unwrap();
/// assert_eq!(entry.check_id, "rust.msrv_defined");
///
/// // Look up by finding code
/// let entry = explain("missing_msrv").unwrap();
/// assert_eq!(entry.code, "missing_msrv");
///
/// // Unknown returns None
/// assert!(explain("unknown_check").is_none());
/// ```
pub fn explain(check_or_code: &str) -> Option<&'static ExplainEntry> {
    // First try exact match on code (most specific)
    if let Some(entry) = EXPLAIN_REGISTRY.iter().find(|e| e.code == check_or_code) {
        return Some(entry);
    }

    // Then try match on check_id (returns first code for that check)
    EXPLAIN_REGISTRY
        .iter()
        .find(|e| e.check_id == check_or_code)
}

/// Get all explanation entries for a specific check ID.
///
/// Returns all finding codes and their explanations for the given check.
///
/// # Arguments
///
/// * `check_id` - The check ID to look up (e.g., "rust.msrv_defined")
///
/// # Returns
///
/// A vector of explain entries for all codes under this check.
pub fn explain_check_all_codes(check_id: &str) -> Vec<&'static ExplainEntry> {
    EXPLAIN_REGISTRY
        .iter()
        .filter(|e| e.check_id == check_id)
        .collect()
}

/// Get all unique check IDs in the registry.
///
/// # Returns
///
/// A sorted vector of all unique check IDs.
pub fn all_check_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = EXPLAIN_REGISTRY.iter().map(|e| e.check_id).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Get all finding codes in the registry.
///
/// # Returns
///
/// A vector of all finding codes.
pub fn all_codes() -> Vec<&'static str> {
    EXPLAIN_REGISTRY.iter().map(|e| e.code).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_by_check_id() {
        let entry = explain("rust.msrv_defined");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().check_id, "rust.msrv_defined");
    }

    #[test]
    fn test_explain_by_code() {
        let entry = explain("missing_msrv");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().code, "missing_msrv");
        assert_eq!(entry.unwrap().check_id, "rust.msrv_defined");
    }

    #[test]
    fn test_explain_unknown() {
        assert!(explain("unknown_check_or_code").is_none());
    }

    #[test]
    fn test_explain_check_all_codes() {
        let entries = explain_check_all_codes("rust.msrv_consistent");
        assert!(!entries.is_empty());
        // Should have multiple codes for msrv_consistent
        assert!(entries.len() >= 2);
        for entry in entries {
            assert_eq!(entry.check_id, "rust.msrv_consistent");
        }
    }

    #[test]
    fn test_all_check_ids() {
        let ids = all_check_ids();
        assert!(!ids.is_empty());
        // Check that some expected IDs are present
        assert!(ids.contains(&"rust.msrv_defined"));
        assert!(ids.contains(&"rust.toolchain_pinning"));
        assert!(ids.contains(&"workspace.resolver_v2"));
    }

    #[test]
    fn test_all_codes() {
        let codes = all_codes();
        assert!(!codes.is_empty());
        // Check that some expected codes are present
        assert!(codes.contains(&"missing_msrv"));
        assert!(codes.contains(&"toolchain_msrv_mismatch"));
    }

    #[test]
    fn test_no_duplicate_codes() {
        let codes = all_codes();
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert!(seen.insert(code));
        }
    }

    #[test]
    fn test_all_entries_have_required_fields() {
        for entry in EXPLAIN_REGISTRY {
            assert!(!entry.check_id.is_empty());
            assert!(!entry.code.is_empty());
            assert!(!entry.name.is_empty());
            assert!(!entry.what_it_means.is_empty());
            assert!(!entry.why_it_matters.is_empty());
            assert!(!entry.how_to_fix.is_empty());
        }
    }

    #[test]
    fn test_check_ids_follow_naming_convention() {
        for entry in EXPLAIN_REGISTRY {
            assert!(entry.check_id.contains('.'));
            let parts: Vec<&str> = entry.check_id.split('.').collect();
            assert_eq!(parts.len(), 2);
        }
    }

    #[test]
    fn test_codes_are_snake_case() {
        for entry in EXPLAIN_REGISTRY {
            assert!(
                entry
                    .code
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            );
        }
    }
}
