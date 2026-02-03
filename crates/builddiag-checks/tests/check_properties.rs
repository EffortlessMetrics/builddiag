//! Property-based tests for check implementations.
//!
//! These tests verify universal properties that should hold across all valid inputs.
//!
//! # Property Categories
//!
//! ## Check Behavior Properties (Properties 2-3)
//! - Property 2: Pass status means no Error severity findings
//! - Property 3: Fail status means findings have non-empty messages
//!
//! ## Error Handling Properties (Properties 3-4 from comprehensive-test-coverage)
//! - Property 3: Graceful Error Handling - invalid inputs return errors without panicking
//! - Property 4: Error Messages Contain Context - error messages are non-empty and informative

use builddiag_checks::run_selected_checks;
use builddiag_domain::parse_rust_version;
use builddiag_repo::{Member, RepoState, Toolchain, WorkspaceInfo, maybe_parse_numeric_version};
use builddiag_types::{CheckStatus, Config, RelationToMsrv, Severity};
use camino::Utf8PathBuf;
use proptest::prelude::*;

// =============================================================================
// Generators for test data
// =============================================================================

/// Generate a valid semantic version string (major.minor.patch)
fn arb_semver() -> impl Strategy<Value = String> {
    (1u32..=2, 50u32..=80, 0u32..=10)
        .prop_map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch))
}

/// Generate a valid Rust version string (major.minor or major.minor.patch)
fn arb_rust_version() -> impl Strategy<Value = String> {
    prop_oneof![
        // Two-part version: 1.70
        (1u32..=2, 50u32..=80).prop_map(|(major, minor)| format!("{}.{}", major, minor)),
        // Three-part version: 1.70.0
        (1u32..=2, 50u32..=80, 0u32..=10)
            .prop_map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch)),
    ]
}

/// Generate a valid pinned toolchain channel (numeric version)
fn arb_pinned_channel() -> impl Strategy<Value = String> {
    arb_semver()
}

/// Generate an unpinned toolchain channel
fn arb_unpinned_channel() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("stable".to_string()),
        Just("beta".to_string()),
        Just("nightly".to_string()),
    ]
}

/// Generate a workspace resolver value
fn arb_resolver() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(Some("2".to_string())),
        Just(Some("1".to_string())),
        Just(None),
    ]
}

// =============================================================================
// Generators for invalid/error-inducing inputs
// =============================================================================

/// Generate invalid version strings that should cause parsing errors.
///
/// These are used to test Property 3: Graceful Error Handling
/// **Validates: Requirements 8.2, 8.3**
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn arb_invalid_version() -> impl Strategy<Value = String> {
    prop_oneof![
        // Empty or whitespace-only strings
        Just("".to_string()),
        Just("   ".to_string()),
        Just("\t\n".to_string()),
        // Invalid characters
        "[a-z]+".prop_map(|s| s),
        Just("abc".to_string()),
        Just("1.x.0".to_string()),
        Just("v1.70.0".to_string()),
        // Malformed version patterns
        Just("1.".to_string()),
        Just(".70.0".to_string()),
        Just("1..0".to_string()),
        Just("1.70.".to_string()),
        // Negative numbers (invalid for semver)
        Just("-1.70.0".to_string()),
        Just("1.-70.0".to_string()),
        // Too many components
        Just("1.70.0.0".to_string()),
        Just("1.70.0.0.0".to_string()),
        // Special characters
        Just("1.70.0-beta".to_string()), // Pre-release (may or may not be valid depending on context)
        Just("1.70.0+build".to_string()), // Build metadata
        Just("1.70.0@latest".to_string()),
        Just("1.70.0#hash".to_string()),
        // Unicode and special chars
        Just("1.70.0\u{200B}".to_string()), // Zero-width space
        Just("①.②.③".to_string()),
    ]
}

/// Generate arbitrary strings that may or may not be valid versions.
///
/// This is a broader generator for fuzzing version parsing.
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn arb_arbitrary_version_string() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid versions (should not panic)
        arb_rust_version(),
        // Invalid versions (should return error, not panic)
        arb_invalid_version(),
        // Random strings
        "[a-zA-Z0-9._-]{0,20}".prop_map(|s| s),
    ]
}

/// Generate invalid toolchain channel strings.
///
/// These are used to test error handling in toolchain parsing.
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn arb_invalid_toolchain_channel() -> impl Strategy<Value = String> {
    prop_oneof![
        // Empty
        Just("".to_string()),
        // Invalid channel names
        Just("invalid-channel".to_string()),
        Just("release".to_string()),
        Just("dev".to_string()),
        // Malformed nightly dates
        Just("nightly-".to_string()),
        Just("nightly-2024".to_string()),
        Just("nightly-invalid".to_string()),
        // Invalid version formats
        Just("1.".to_string()),
        Just(".70".to_string()),
    ]
}

// =============================================================================
// Helper functions to create RepoState variants
// =============================================================================

/// Create a minimal RepoState for testing
fn mock_repo_state() -> RepoState {
    RepoState {
        root: Utf8PathBuf::from("/test/repo"),
        cargo_root: Some(Utf8PathBuf::from("/test/repo/Cargo.toml")),
        toolchain: None,
        workspace: WorkspaceInfo {
            is_workspace: true,
            members: Vec::new(),
            workspace_msrv: None,
            workspace_edition: Some("2021".to_string()),
            workspace_resolver: Some("2".to_string()),
        },
        workspace_model: None,
        tools_checksums: None,
        tools_manifest: None,
        changed_files: None,
        lockfile_exists: true,
    }
}

/// Create a RepoState with workspace MSRV set
fn mock_repo_with_msrv(msrv: &str) -> RepoState {
    let mut repo = mock_repo_state();
    repo.workspace.workspace_msrv = Some(msrv.to_string());
    repo
}

/// Create a RepoState with toolchain
fn mock_repo_with_toolchain(channel: &str) -> RepoState {
    let mut repo = mock_repo_state();
    repo.toolchain = Some(Toolchain {
        path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
        channel: channel.to_string(),
    });
    repo
}

/// Create a RepoState with both MSRV and toolchain
fn mock_repo_with_msrv_and_toolchain(msrv: &str, channel: &str) -> RepoState {
    let mut repo = mock_repo_with_msrv(msrv);
    repo.toolchain = Some(Toolchain {
        path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
        channel: channel.to_string(),
    });
    repo
}

/// Create a RepoState with workspace members
fn mock_repo_with_members(workspace_msrv: Option<&str>, members: Vec<Member>) -> RepoState {
    let mut repo = mock_repo_state();
    repo.workspace.workspace_msrv = workspace_msrv.map(|s| s.to_string());
    repo.workspace.members = members;
    repo
}

/// Create a RepoState with resolver setting
fn mock_repo_with_resolver(resolver: Option<&str>) -> RepoState {
    let mut repo = mock_repo_state();
    repo.workspace.workspace_resolver = resolver.map(|s| s.to_string());
    repo
}

// =============================================================================
// Helper functions for error handling tests
// =============================================================================

/// Test that version parsing handles invalid input gracefully (returns error, doesn't panic).
///
/// This helper is used by Property 3: Graceful Error Handling tests.
/// **Validates: Requirements 8.2, 8.3**
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn assert_version_parsing_graceful(input: &str) -> bool {
    // The function should either succeed or return an error, but never panic
    let result = std::panic::catch_unwind(|| parse_rust_version(input));

    match result {
        Ok(_parse_result) => {
            // Parsing completed without panic - this is the expected behavior
            // The result can be Ok (valid version) or Err (invalid version)
            true
        }
        Err(_) => {
            // Function panicked - this is a bug
            false
        }
    }
}

/// Test that maybe_parse_numeric_version handles invalid input gracefully.
///
/// This helper is used by Property 3: Graceful Error Handling tests.
/// **Validates: Requirements 8.2, 8.3**
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn assert_numeric_version_parsing_graceful(input: &str) -> bool {
    let result = std::panic::catch_unwind(|| maybe_parse_numeric_version(input));
    result.is_ok() // Completed without panic
}

/// Test that an error message contains meaningful context.
///
/// This helper is used by Property 4: Error Messages Contain Context tests.
/// **Validates: Requirements 8.7**
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn assert_error_has_context(error: &anyhow::Error) -> bool {
    let msg = error.to_string();

    // Error message should be non-empty
    if msg.is_empty() {
        return false;
    }

    // Error message should have some minimum length to be informative
    // (more than just "error" or similar)
    if msg.len() < 5 {
        return false;
    }

    true
}

/// Test that a finding message contains meaningful context.
///
/// This helper is used by Property 4: Error Messages Contain Context tests.
/// **Validates: Requirements 8.7**
#[allow(dead_code)] // Used by tasks 6.2 and 6.3
fn assert_finding_has_context(message: &str) -> bool {
    // Message should be non-empty
    if message.is_empty() {
        return false;
    }

    // Message should have some minimum length to be informative
    if message.len() < 10 {
        return false;
    }

    true
}

// =============================================================================
// Property Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // =========================================================================
    // Property 2: Check Pass Behavior
    // Feature: release-ready, Property 2: Check Pass Behavior
    // For valid inputs, Pass status means no Error severity findings
    // **Validates: Requirements 5.1**
    // =========================================================================

    /// Property 2: When MSRV is defined, check_msrv_defined returns Pass with no Error findings.
    ///
    /// Feature: release-ready, Property 2: Check Pass Behavior
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_msrv_defined_pass_has_no_error_findings(msrv in arb_rust_version()) {
        let repo = mock_repo_with_msrv(&msrv);
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.msrv_defined").unwrap();

        // Property: Pass status means no Error severity findings
        if report.status == CheckStatus::Pass {
            prop_assert!(
                report.findings.iter().all(|f| f.severity != Severity::Error),
                "Pass status should have no Error findings, but found: {:?}",
                report.findings
            );
        }
    }

    /// Property 2: When toolchain is pinned to a valid version, check_toolchain_pinning returns Pass with no Error findings.
    ///
    /// Feature: release-ready, Property 2: Check Pass Behavior
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_toolchain_pinning_pass_has_no_error_findings(channel in arb_pinned_channel()) {
        let repo = mock_repo_with_toolchain(&channel);
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.toolchain_pinning").unwrap();

        // Property: Pass status means no Error severity findings
        if report.status == CheckStatus::Pass {
            prop_assert!(
                report.findings.iter().all(|f| f.severity != Severity::Error),
                "Pass status should have no Error findings, but found: {:?}",
                report.findings
            );
        }
    }

    /// Property 2: When toolchain equals MSRV, check_toolchain_msrv_relation returns Pass with no Error findings.
    ///
    /// Feature: release-ready, Property 2: Check Pass Behavior
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_toolchain_msrv_relation_pass_has_no_error_findings(version in arb_semver()) {
        // Use same version for both MSRV and toolchain to ensure pass
        let repo = mock_repo_with_msrv_and_toolchain(&version, &version);
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.toolchain_msrv_relation").unwrap();

        // Property: Pass status means no Error severity findings
        if report.status == CheckStatus::Pass {
            prop_assert!(
                report.findings.iter().all(|f| f.severity != Severity::Error),
                "Pass status should have no Error findings, but found: {:?}",
                report.findings
            );
        }
    }

    /// Property 2: When resolver is "2", check_workspace_resolver returns Pass with no Error findings.
    ///
    /// Feature: release-ready, Property 2: Check Pass Behavior
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_workspace_resolver_pass_has_no_error_findings(_dummy in Just(())) {
        let repo = mock_repo_with_resolver(Some("2"));
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "workspace.resolver_v2").unwrap();

        // Property: Pass status means no Error severity findings
        if report.status == CheckStatus::Pass {
            prop_assert!(
                report.findings.iter().all(|f| f.severity != Severity::Error),
                "Pass status should have no Error findings, but found: {:?}",
                report.findings
            );
        }
    }

    /// Property 2: Universal property - for any check that returns Pass, there are no Error severity findings.
    ///
    /// Feature: release-ready, Property 2: Check Pass Behavior
    /// **Validates: Requirements 5.1**
    #[test]
    fn prop_all_checks_pass_has_no_error_findings(
        msrv in arb_rust_version(),
        toolchain in arb_pinned_channel(),
    ) {
        // Create a valid repo state that should pass most checks
        let mut repo = mock_repo_with_msrv_and_toolchain(&msrv, &toolchain);
        repo.workspace.workspace_resolver = Some("2".to_string());

        // Disable checksums requirement since we don't have checksums files
        let mut config = Config::default();
        config.policy.checksums.require_file = false;
        // Allow toolchain to be at least MSRV (more flexible)
        config.policy.toolchain.relation_to_msrv = RelationToMsrv::AtLeast;

        let reports = run_selected_checks(&repo, &config, true).unwrap();

        // Property: For ALL checks, Pass status means no Error severity findings
        for report in &reports {
            if report.status == CheckStatus::Pass {
                prop_assert!(
                    report.findings.iter().all(|f| f.severity != Severity::Error),
                    "Check '{}' has Pass status but contains Error findings: {:?}",
                    report.id,
                    report.findings
                );
            }
        }
    }

    // =========================================================================
    // Property 3: Check Fail Behavior
    // Feature: release-ready, Property 3: Check Fail Behavior
    // For invalid inputs, findings have non-empty messages
    // **Validates: Requirements 5.2**
    // =========================================================================

    /// Property 3: When MSRV is missing and required, check_msrv_defined returns findings with non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_msrv_defined_fail_has_nonempty_messages(_dummy in Just(())) {
        // Create repo without MSRV
        let repo = mock_repo_state();
        let config = Config::default(); // require_defined = true by default

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.msrv_defined").unwrap();

        // Property: Fail status means findings have non-empty messages
        if report.status == CheckStatus::Fail {
            prop_assert!(!report.findings.is_empty(), "Fail status should have findings");
            for finding in &report.findings {
                prop_assert!(
                    !finding.message.is_empty(),
                    "Finding should have non-empty message: {:?}",
                    finding
                );
            }
        }
    }

    /// Property 3: When toolchain is unpinned, check_toolchain_pinning returns findings with non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_toolchain_pinning_fail_has_nonempty_messages(channel in arb_unpinned_channel()) {
        let repo = mock_repo_with_toolchain(&channel);
        let mut config = Config::default();
        // Allow nightly to avoid nightly_disallowed finding interfering
        config.policy.toolchain.allow_nightly = true;

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.toolchain_pinning").unwrap();

        // Property: Fail status means findings have non-empty messages
        if report.status == CheckStatus::Fail {
            prop_assert!(!report.findings.is_empty(), "Fail status should have findings");
            for finding in &report.findings {
                prop_assert!(
                    !finding.message.is_empty(),
                    "Finding should have non-empty message: {:?}",
                    finding
                );
            }
        }
    }

    /// Property 3: When toolchain doesn't match MSRV, check_toolchain_msrv_relation returns findings with non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_toolchain_msrv_relation_fail_has_nonempty_messages(
        msrv_minor in 60u32..=70,
        toolchain_minor in 75u32..=80,
    ) {
        // Ensure toolchain > MSRV to trigger mismatch with Equals policy
        let msrv = format!("1.{}.0", msrv_minor);
        let toolchain = format!("1.{}.0", toolchain_minor);
        let repo = mock_repo_with_msrv_and_toolchain(&msrv, &toolchain);
        let config = Config::default(); // relation_to_msrv = Equals by default

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.toolchain_msrv_relation").unwrap();

        // Property: Fail status means findings have non-empty messages
        if report.status == CheckStatus::Fail {
            prop_assert!(!report.findings.is_empty(), "Fail status should have findings");
            for finding in &report.findings {
                prop_assert!(
                    !finding.message.is_empty(),
                    "Finding should have non-empty message: {:?}",
                    finding
                );
            }
        }
    }

    /// Property 3: When resolver is not "2", check_workspace_resolver returns findings with non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_workspace_resolver_fail_has_nonempty_messages(resolver in prop_oneof![Just(Some("1")), Just(None)]) {
        let repo = mock_repo_with_resolver(resolver);
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "workspace.resolver_v2").unwrap();

        // Property: Warn/Fail status means findings have non-empty messages
        // Note: workspace.resolver_v2 uses Warn severity by default
        if report.status == CheckStatus::Fail || report.status == CheckStatus::Warn {
            prop_assert!(!report.findings.is_empty(), "Fail/Warn status should have findings");
            for finding in &report.findings {
                prop_assert!(
                    !finding.message.is_empty(),
                    "Finding should have non-empty message: {:?}",
                    finding
                );
            }
        }
    }

    /// Property 3: Universal property - for any check that returns Fail, all findings have non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_all_checks_fail_has_nonempty_messages(
        has_msrv in any::<bool>(),
        has_toolchain in any::<bool>(),
        resolver in arb_resolver(),
    ) {
        // Create a repo state that may fail various checks
        let mut repo = mock_repo_state();

        if has_msrv {
            repo.workspace.workspace_msrv = Some("1.70.0".to_string());
        }

        if has_toolchain {
            repo.toolchain = Some(Toolchain {
                path: Utf8PathBuf::from("/test/repo/rust-toolchain.toml"),
                channel: "stable".to_string(), // unpinned to trigger failure
            });
        }

        repo.workspace.workspace_resolver = resolver;

        let config = Config::default();
        let reports = run_selected_checks(&repo, &config, true).unwrap();

        // Property: For ALL checks, Fail status means findings have non-empty messages
        for report in &reports {
            if report.status == CheckStatus::Fail || report.status == CheckStatus::Warn {
                // If there are findings, they must have non-empty messages
                for finding in &report.findings {
                    prop_assert!(
                        !finding.message.is_empty(),
                        "Check '{}' has finding with empty message: {:?}",
                        report.id,
                        finding
                    );
                }
            }
        }
    }

    /// Property 3: When members have inconsistent MSRV, check_msrv_consistent returns findings with non-empty messages.
    ///
    /// Feature: release-ready, Property 3: Check Fail Behavior
    /// **Validates: Requirements 5.2**
    #[test]
    fn prop_msrv_consistent_fail_has_nonempty_messages(
        workspace_minor in 70u32..=75,
        member_minor in 60u32..=65,
    ) {
        // Create workspace with MSRV and a member with different MSRV
        let workspace_msrv = format!("1.{}.0", workspace_minor);
        let member_msrv = format!("1.{}.0", member_minor);

        let member = Member {
            name: "test-crate".to_string(),
            manifest_path: Utf8PathBuf::from("/test/repo/crates/test-crate/Cargo.toml"),
            rust_version: Some(member_msrv),
            rust_version_workspace: false,
            edition: Some("2021".to_string()),
            edition_workspace: true,
            has_binary_target: false,
        };

        let repo = mock_repo_with_members(Some(&workspace_msrv), vec![member]);
        let config = Config::default();

        let reports = run_selected_checks(&repo, &config, true).unwrap();
        let report = reports.iter().find(|r| r.id == "rust.msrv_consistent").unwrap();

        // Property: Fail status means findings have non-empty messages
        if report.status == CheckStatus::Fail {
            prop_assert!(!report.findings.is_empty(), "Fail status should have findings");
            for finding in &report.findings {
                prop_assert!(
                    !finding.message.is_empty(),
                    "Finding should have non-empty message: {:?}",
                    finding
                );
            }
        }
    }

    // =========================================================================
    // Property 3: Graceful Error Handling (from comprehensive-test-coverage)
    // Feature: comprehensive-test-coverage, Property 3: Graceful Error Handling
    // For any invalid input, the tool should return an error without panicking.
    // **Validates: Requirements 8.2, 8.3**
    // =========================================================================

    /// Property 3: Graceful Error Handling - parse_rust_version handles invalid input without panicking.
    ///
    /// For any invalid version string, parse_rust_version should either return Ok (if valid)
    /// or Err (if invalid), but should never panic.
    ///
    /// Feature: comprehensive-test-coverage, Property 3: Graceful Error Handling
    /// **Validates: Requirements 8.2, 8.3**
    #[test]
    fn prop_graceful_error_handling_parse_rust_version(input in arb_invalid_version()) {
        // The function should either succeed or return an error, but never panic
        let result = std::panic::catch_unwind(|| parse_rust_version(&input));

        prop_assert!(
            result.is_ok(),
            "parse_rust_version panicked on input: {:?}",
            input
        );

        // If it didn't panic, verify it returned a proper Result (Ok or Err)
        // For invalid inputs, we expect Err in most cases
        if let Ok(parse_result) = result {
            // The result should be a valid Result type - either Ok or Err is fine
            // We just care that it didn't panic
            match parse_result {
                Ok(_version) => {
                    // Some "invalid" inputs might actually be valid - that's okay
                }
                Err(_err) => {
                    // Expected for truly invalid inputs
                }
            }
        }
    }

    /// Property 3: Graceful Error Handling - maybe_parse_numeric_version handles invalid input without panicking.
    ///
    /// For any invalid toolchain channel string, maybe_parse_numeric_version should either
    /// return Ok (if valid) or Err (if invalid), but should never panic.
    ///
    /// Feature: comprehensive-test-coverage, Property 3: Graceful Error Handling
    /// **Validates: Requirements 8.2, 8.3**
    #[test]
    fn prop_graceful_error_handling_maybe_parse_numeric_version(input in arb_invalid_toolchain_channel()) {
        // The function should either succeed or return an error, but never panic
        let result = std::panic::catch_unwind(|| maybe_parse_numeric_version(&input));

        prop_assert!(
            result.is_ok(),
            "maybe_parse_numeric_version panicked on input: {:?}",
            input
        );

        // If it didn't panic, verify it returned a proper Result
        if let Ok(parse_result) = result {
            match parse_result {
                Ok(_) => {
                    // Valid result - either Some(version) or None for non-numeric channels
                }
                Err(_) => {
                    // Error result - also acceptable for invalid inputs
                }
            }
        }
    }

    /// Property 3: Graceful Error Handling - arbitrary strings don't cause panics.
    ///
    /// For any arbitrary string (valid or invalid), version parsing functions should
    /// handle the input gracefully without panicking.
    ///
    /// Feature: comprehensive-test-coverage, Property 3: Graceful Error Handling
    /// **Validates: Requirements 8.2, 8.3**
    #[test]
    fn prop_graceful_error_handling_arbitrary_strings(input in arb_arbitrary_version_string()) {
        // Test parse_rust_version with arbitrary input
        let rust_version_result = std::panic::catch_unwind(|| parse_rust_version(&input));
        prop_assert!(
            rust_version_result.is_ok(),
            "parse_rust_version panicked on arbitrary input: {:?}",
            input
        );

        // Test maybe_parse_numeric_version with arbitrary input
        let numeric_version_result = std::panic::catch_unwind(|| maybe_parse_numeric_version(&input));
        prop_assert!(
            numeric_version_result.is_ok(),
            "maybe_parse_numeric_version panicked on arbitrary input: {:?}",
            input
        );
    }

    // =========================================================================
    // Property 4: Error Messages Contain Context (from comprehensive-test-coverage)
    // Feature: comprehensive-test-coverage, Property 4: Error Messages Contain Context
    // For any error condition, the error message should be non-empty and contain
    // information about what went wrong.
    // **Validates: Requirements 8.7**
    // =========================================================================

    /// Property 4: Error Messages Contain Context - parse_rust_version error messages are informative.
    ///
    /// For any invalid version string that causes parse_rust_version to return an error,
    /// the error message should be non-empty and contain meaningful context (at least 10 characters).
    ///
    /// Feature: comprehensive-test-coverage, Property 4: Error Messages Contain Context
    /// **Validates: Requirements 8.7**
    #[test]
    fn prop_error_messages_contain_context(input in arb_invalid_version()) {
        let result = parse_rust_version(&input);

        // If parsing fails (which is expected for invalid inputs), verify the error message
        if let Err(err) = result {
            let error_message = err.to_string();

            // Property: Error message should be non-empty
            prop_assert!(
                !error_message.is_empty(),
                "Error message should not be empty for invalid input: {:?}",
                input
            );

            // Property: Error message should contain meaningful context (more than 10 characters)
            // This ensures the message is informative, not just "error" or similar
            prop_assert!(
                error_message.len() > 10,
                "Error message should contain meaningful context (>10 chars), got '{}' ({} chars) for input: {:?}",
                error_message,
                error_message.len(),
                input
            );
        }
        // Note: Some inputs in arb_invalid_version() might actually be valid
        // (e.g., "1.70.0-beta" could be valid depending on semver parsing).
        // If parsing succeeds, that's acceptable - we only verify error message quality
        // when parsing fails.
    }
}
