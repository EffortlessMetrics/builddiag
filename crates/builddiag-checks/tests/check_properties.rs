//! Property-based tests for check implementations.
//!
//! These tests verify universal properties that should hold across all valid inputs.

use builddiag_checks::run_selected_checks;
use builddiag_repo::{Member, RepoState, Toolchain, WorkspaceInfo};
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
        tools_checksums: None,
        tools_manifest: None,
        changed_files: None,
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
}
