//! Property-based tests for builddiag-types.
//!
//! This module contains property tests that validate universal invariants
//! for the core types in builddiag-types, particularly focusing on
//! serialization round-trips.
//!
//! # Properties Tested
//!
//! - **Property 1**: Config Serialization Round-Trip (Requirements 3.8)
//! - **Property 2**: Report Serialization Round-Trip (Requirements 3.9, 8.5)

use builddiag_types::{
    CheckConfig, CheckReport, CheckStatus, ChecksumsPolicy, Config, Defaults, FailOn, Finding,
    Inputs, MsrvPolicy, MsrvSource, PathsConfig, Policy, RelationToMsrv, RepoDetected, RepoInfo,
    Report, RunInfo, SchemaId, Severity, Summary, SummaryCounts, ToolInfo, ToolchainPolicy,
    Verdict,
};
use chrono::{TimeZone, Utc};
use proptest::prelude::*;

// =============================================================================
// Proptest Configuration
// =============================================================================

/// Configure proptest to run at least 100 iterations per property
/// as specified in the design document.
const PROPTEST_CASES: u32 = 100;

// =============================================================================
// Arbitrary Generators
// =============================================================================

/// Generate arbitrary non-empty strings suitable for identifiers and paths.
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,20}".prop_map(|s| s.to_string())
}

/// Generate arbitrary human-readable messages.
fn arb_message() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?-]{1,100}".prop_map(|s| s.to_string())
}

/// Generate arbitrary file paths.
fn arb_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/]{0,30}\\.(toml|rs|md)".prop_map(|s| s.to_string())
}

/// Generate arbitrary version strings.
fn arb_version() -> impl Strategy<Value = String> {
    (0u32..10, 0u32..100, 0u32..100).prop_map(|(maj, min, pat)| format!("{}.{}.{}", maj, min, pat))
}

/// Generate arbitrary Severity values.
fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

/// Generate arbitrary CheckStatus values.
fn arb_check_status() -> impl Strategy<Value = CheckStatus> {
    prop_oneof![
        Just(CheckStatus::Pass),
        Just(CheckStatus::Warn),
        Just(CheckStatus::Fail),
        Just(CheckStatus::Skip),
    ]
}

/// Generate arbitrary Verdict values.
fn arb_verdict() -> impl Strategy<Value = Verdict> {
    prop_oneof![
        Just(Verdict::Pass),
        Just(Verdict::Warn),
        Just(Verdict::Fail),
        Just(Verdict::Skip),
    ]
}

/// Generate arbitrary FailOn values.
fn arb_fail_on() -> impl Strategy<Value = FailOn> {
    prop_oneof![Just(FailOn::Error), Just(FailOn::Warn), Just(FailOn::Never),]
}

/// Generate arbitrary MsrvSource values.
fn arb_msrv_source() -> impl Strategy<Value = MsrvSource> {
    prop_oneof![Just(MsrvSource::Workspace), Just(MsrvSource::Any),]
}

/// Generate arbitrary RelationToMsrv values.
fn arb_relation_to_msrv() -> impl Strategy<Value = RelationToMsrv> {
    prop_oneof![Just(RelationToMsrv::Equals), Just(RelationToMsrv::AtLeast),]
}

/// Generate arbitrary Finding instances.
fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_severity(),
        arb_identifier(),
        arb_message(),
        proptest::option::of(arb_path()),
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(|(severity, code, message, path, line, column)| Finding {
            severity,
            code,
            message,
            path,
            line,
            column,
        })
}

/// Generate arbitrary CheckReport instances.
fn arb_check_report() -> impl Strategy<Value = CheckReport> {
    (
        arb_identifier(),
        arb_check_status(),
        proptest::collection::vec(arb_finding(), 0..5),
        proptest::option::of(arb_message()),
    )
        .prop_map(|(id, status, findings, skipped_reason)| CheckReport {
            id,
            status,
            findings,
            skipped_reason,
        })
}

/// Generate arbitrary SummaryCounts instances.
fn arb_summary_counts() -> impl Strategy<Value = SummaryCounts> {
    (0usize..100, 0usize..100, 0usize..100).prop_map(|(info, warn, error)| SummaryCounts {
        info,
        warn,
        error,
    })
}

/// Generate arbitrary Summary instances.
fn arb_summary() -> impl Strategy<Value = Summary> {
    (
        arb_summary_counts(),
        arb_verdict(),
        proptest::collection::vec(arb_message(), 0..5),
    )
        .prop_map(|(counts, verdict, reasons)| Summary {
            counts,
            verdict,
            reasons,
        })
}

/// Generate arbitrary ToolInfo instances.
fn arb_tool_info() -> impl Strategy<Value = ToolInfo> {
    (arb_identifier(), arb_version()).prop_map(|(name, version)| ToolInfo { name, version })
}

/// Generate a valid UTC timestamp within a reasonable range.
fn arb_datetime() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    // Generate timestamps between 2020 and 2030
    (
        2020i32..2030,
        1u32..13,
        1u32..29,
        0u32..24,
        0u32..60,
        0u32..60,
    )
        .prop_map(|(year, month, day, hour, min, sec)| {
            Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
                .single()
                .unwrap_or_else(|| Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap())
        })
}

/// Generate arbitrary RunInfo instances.
fn arb_run_info() -> impl Strategy<Value = RunInfo> {
    (
        arb_identifier(),
        arb_datetime(),
        proptest::option::of(arb_datetime()),
    )
        .prop_map(|(id, started_at, ended_at)| RunInfo {
            id,
            started_at,
            ended_at,
        })
}

/// Generate arbitrary RepoDetected instances.
fn arb_repo_detected() -> impl Strategy<Value = RepoDetected> {
    (any::<bool>(), 1usize..20).prop_map(|(is_workspace, members)| RepoDetected {
        is_workspace,
        members,
    })
}

/// Generate arbitrary RepoInfo instances.
fn arb_repo_info() -> impl Strategy<Value = RepoInfo> {
    (arb_path(), arb_repo_detected()).prop_map(|(root, detected)| RepoInfo { root, detected })
}

/// Generate arbitrary Inputs instances.
fn arb_inputs() -> impl Strategy<Value = Inputs> {
    (
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
        proptest::option::of(arb_path()),
    )
        .prop_map(
            |(cargo_root, rust_toolchain, tools_checksums, tools_manifest)| Inputs {
                cargo_root,
                rust_toolchain,
                tools_checksums,
                tools_manifest,
            },
        )
}

/// Generate arbitrary SchemaId instances.
fn arb_schema_id() -> impl Strategy<Value = SchemaId> {
    arb_identifier().prop_map(SchemaId)
}

/// Generate arbitrary Report instances.
fn arb_report() -> impl Strategy<Value = Report> {
    (
        arb_schema_id(),
        arb_tool_info(),
        arb_run_info(),
        arb_repo_info(),
        arb_inputs(),
        proptest::collection::vec(arb_check_report(), 0..10),
        arb_summary(),
    )
        .prop_map(
            |(schema, tool, run, repo, inputs, checks, summary)| Report {
                schema,
                tool,
                run,
                repo,
                inputs,
                checks,
                summary,
            },
        )
}

// =============================================================================
// Config Generators
// =============================================================================

/// Generate arbitrary Defaults instances.
fn arb_defaults() -> impl Strategy<Value = Defaults> {
    (
        arb_fail_on(),
        arb_path(),
        any::<bool>(),
        arb_identifier(),
        arb_identifier(),
    )
        .prop_map(|(fail_on, out_dir, diff_aware, base, head)| Defaults {
            fail_on,
            out_dir,
            diff_aware,
            base,
            head,
        })
}

/// Generate arbitrary PathsConfig instances.
fn arb_paths_config() -> impl Strategy<Value = PathsConfig> {
    (arb_path(), arb_path(), arb_path(), arb_path()).prop_map(
        |(cargo_root, rust_toolchain, tools_checksums, tools_manifest)| PathsConfig {
            cargo_root,
            rust_toolchain,
            tools_checksums,
            tools_manifest,
        },
    )
}

/// Generate arbitrary MsrvPolicy instances.
fn arb_msrv_policy() -> impl Strategy<Value = MsrvPolicy> {
    (
        any::<bool>(),
        arb_msrv_source(),
        any::<bool>(),
        proptest::collection::vec(arb_identifier(), 0..5),
    )
        .prop_map(
            |(require_defined, source, allow_per_crate_override, allow_overrides)| MsrvPolicy {
                require_defined,
                source,
                allow_per_crate_override,
                allow_overrides,
            },
        )
}

/// Generate arbitrary ToolchainPolicy instances.
fn arb_toolchain_policy() -> impl Strategy<Value = ToolchainPolicy> {
    (any::<bool>(), arb_relation_to_msrv(), any::<bool>()).prop_map(
        |(require_pinned, relation_to_msrv, allow_nightly)| ToolchainPolicy {
            require_pinned,
            relation_to_msrv,
            allow_nightly,
        },
    )
}

/// Generate arbitrary ChecksumsPolicy instances.
fn arb_checksums_policy() -> impl Strategy<Value = ChecksumsPolicy> {
    (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(require_file, require_coverage, verify_local_files)| ChecksumsPolicy {
            require_file,
            require_coverage,
            verify_local_files,
        },
    )
}

/// Generate arbitrary Policy instances.
fn arb_policy() -> impl Strategy<Value = Policy> {
    (
        arb_msrv_policy(),
        arb_toolchain_policy(),
        arb_checksums_policy(),
    )
        .prop_map(|(msrv, toolchain, checksums)| Policy {
            msrv,
            toolchain,
            checksums,
        })
}

/// Generate arbitrary CheckConfig instances.
fn arb_check_config() -> impl Strategy<Value = CheckConfig> {
    (
        arb_identifier(),
        arb_severity(),
        any::<bool>(),
        proptest::collection::vec(arb_path(), 0..3),
    )
        .prop_map(|(id, severity, enabled, triggers)| CheckConfig {
            id,
            severity,
            enabled,
            triggers,
        })
}

/// Generate arbitrary Config instances.
fn arb_config() -> impl Strategy<Value = Config> {
    (
        arb_defaults(),
        arb_paths_config(),
        arb_policy(),
        proptest::collection::vec(arb_check_config(), 0..5),
        proptest::collection::btree_map(arb_identifier(), arb_message(), 0..5),
    )
        .prop_map(|(defaults, paths, policy, checks, meta)| Config {
            defaults,
            paths,
            policy,
            checks,
            meta,
        })
}

// =============================================================================
// Property Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

    // =========================================================================
    // Property 1: Config Serialization Round-Trip
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 1: Config Serialization Round-Trip
    ///
    /// For any valid Config instance, serializing it to TOML and then parsing
    /// it back should produce an equivalent Config.
    ///
    /// **Validates: Requirements 3.8**
    #[test]
    fn prop_config_toml_roundtrip(config in arb_config()) {
        let toml_str = toml::to_string(&config).expect("Config should serialize to TOML");
        let parsed: Config = toml::from_str(&toml_str).expect("TOML should parse back to Config");

        // Compare all fields
        prop_assert_eq!(config.defaults.fail_on, parsed.defaults.fail_on);
        prop_assert_eq!(config.defaults.out_dir, parsed.defaults.out_dir);
        prop_assert_eq!(config.defaults.diff_aware, parsed.defaults.diff_aware);
        prop_assert_eq!(config.defaults.base, parsed.defaults.base);
        prop_assert_eq!(config.defaults.head, parsed.defaults.head);

        prop_assert_eq!(config.paths.cargo_root, parsed.paths.cargo_root);
        prop_assert_eq!(config.paths.rust_toolchain, parsed.paths.rust_toolchain);
        prop_assert_eq!(config.paths.tools_checksums, parsed.paths.tools_checksums);
        prop_assert_eq!(config.paths.tools_manifest, parsed.paths.tools_manifest);

        prop_assert_eq!(config.policy.msrv.require_defined, parsed.policy.msrv.require_defined);
        prop_assert_eq!(config.policy.msrv.source, parsed.policy.msrv.source);
        prop_assert_eq!(config.policy.msrv.allow_per_crate_override, parsed.policy.msrv.allow_per_crate_override);
        prop_assert_eq!(config.policy.msrv.allow_overrides, parsed.policy.msrv.allow_overrides);

        prop_assert_eq!(config.policy.toolchain.require_pinned, parsed.policy.toolchain.require_pinned);
        prop_assert_eq!(config.policy.toolchain.relation_to_msrv, parsed.policy.toolchain.relation_to_msrv);
        prop_assert_eq!(config.policy.toolchain.allow_nightly, parsed.policy.toolchain.allow_nightly);

        prop_assert_eq!(config.policy.checksums.require_file, parsed.policy.checksums.require_file);
        prop_assert_eq!(config.policy.checksums.require_coverage, parsed.policy.checksums.require_coverage);
        prop_assert_eq!(config.policy.checksums.verify_local_files, parsed.policy.checksums.verify_local_files);

        prop_assert_eq!(config.checks.len(), parsed.checks.len());
        prop_assert_eq!(config.meta, parsed.meta);
    }

    // =========================================================================
    // Property 2: Report Serialization Round-Trip
    // =========================================================================

    /// Feature: comprehensive-test-coverage, Property 2: Report Serialization Round-Trip
    ///
    /// For any valid Report instance, serializing it to JSON and then parsing
    /// it back should produce an equivalent Report.
    ///
    /// **Validates: Requirements 3.9, 8.5**
    #[test]
    fn prop_report_json_roundtrip(report in arb_report()) {
        let json_str = serde_json::to_string(&report).expect("Report should serialize to JSON");
        let parsed: Report = serde_json::from_str(&json_str).expect("JSON should parse back to Report");

        // Verify the round-trip produces equivalent data
        prop_assert_eq!(report.schema.0, parsed.schema.0);
        prop_assert_eq!(report.tool.name, parsed.tool.name);
        prop_assert_eq!(report.tool.version, parsed.tool.version);
        prop_assert_eq!(report.run.id, parsed.run.id);
        prop_assert_eq!(report.run.started_at, parsed.run.started_at);
        prop_assert_eq!(report.run.ended_at, parsed.run.ended_at);
        prop_assert_eq!(report.repo.root, parsed.repo.root);
        prop_assert_eq!(report.repo.detected.is_workspace, parsed.repo.detected.is_workspace);
        prop_assert_eq!(report.repo.detected.members, parsed.repo.detected.members);
        prop_assert_eq!(report.inputs, parsed.inputs);
        prop_assert_eq!(report.checks.len(), parsed.checks.len());
        prop_assert_eq!(report.summary.counts, parsed.summary.counts);
        prop_assert_eq!(report.summary.verdict, parsed.summary.verdict);
        prop_assert_eq!(report.summary.reasons, parsed.summary.reasons);
    }

    /// Additional test: Report JSON output is always valid JSON
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_report_produces_valid_json(report in arb_report()) {
        let json_str = serde_json::to_string(&report).expect("Report should serialize to JSON");

        // Verify it's valid JSON by parsing as generic Value
        let _: serde_json::Value = serde_json::from_str(&json_str)
            .expect("Serialized report should be valid JSON");
    }

    /// Additional test: Config JSON round-trip (alternative serialization format)
    ///
    /// **Validates: Requirements 3.8**
    #[test]
    fn prop_config_json_roundtrip(config in arb_config()) {
        let json_str = serde_json::to_string(&config).expect("Config should serialize to JSON");
        let parsed: Config = serde_json::from_str(&json_str).expect("JSON should parse back to Config");

        // Verify key fields match
        prop_assert_eq!(config.defaults.fail_on, parsed.defaults.fail_on);
        prop_assert_eq!(config.paths.cargo_root, parsed.paths.cargo_root);
        prop_assert_eq!(config.policy.msrv.source, parsed.policy.msrv.source);
        prop_assert_eq!(config.meta, parsed.meta);
    }
}
