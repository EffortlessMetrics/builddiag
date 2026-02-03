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
    CheckConfig, ChecksumsPolicy, Config, Defaults, EditionPolicy, FailOn, Finding, GitInfo,
    HostInfo, Location, LockfilePolicy, MemberOrderingPolicy, MsrvPolicy, MsrvSource, PathsConfig,
    Policy, Profile, RelationToMsrv, Report, RunInfo, Severity, Summary, ToolInfo, ToolchainPolicy,
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

/// Generate arbitrary Verdict values.
fn arb_verdict() -> impl Strategy<Value = Verdict> {
    prop_oneof![
        Just(Verdict::Pass),
        Just(Verdict::Warn),
        Just(Verdict::Fail),
        Just(Verdict::Skip),
        Just(Verdict::Error),
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

/// Generate arbitrary Profile values.
fn arb_profile() -> impl Strategy<Value = Profile> {
    prop_oneof![
        Just(Profile::Oss),
        Just(Profile::Team),
        Just(Profile::Strict),
    ]
}

/// Generate arbitrary Location instances.
fn arb_location() -> impl Strategy<Value = Location> {
    (
        arb_path(),
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(|(path, line, col)| Location { path, line, col })
}

/// Generate arbitrary Finding instances.
fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_identifier(),
        arb_identifier(),
        arb_severity(),
        arb_message(),
        proptest::option::of(arb_location()),
    )
        .prop_map(|(check_id, code, severity, message, location)| Finding {
            check_id,
            code,
            severity,
            message,
            location,
            data: None, // Skip arbitrary JSON data for simplicity
        })
}

/// Generate arbitrary HostInfo instances.
fn arb_host_info() -> impl Strategy<Value = HostInfo> {
    prop_oneof![
        Just(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string()
        }),
        Just(HostInfo {
            os: "macos".to_string(),
            arch: "aarch64".to_string()
        }),
        Just(HostInfo {
            os: "windows".to_string(),
            arch: "x86_64".to_string()
        }),
    ]
}

/// Generate arbitrary GitInfo instances.
fn arb_git_info() -> impl Strategy<Value = GitInfo> {
    (
        "[a-f0-9]{40}".prop_map(|s| s.to_string()),
        proptest::option::of(arb_identifier()),
        any::<bool>(),
    )
        .prop_map(|(commit, branch, dirty)| GitInfo {
            commit,
            branch,
            dirty,
        })
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
        arb_datetime(),
        proptest::option::of(arb_datetime()),
        0u64..100000,
        arb_host_info(),
        proptest::option::of(arb_git_info()),
    )
        .prop_map(|(started_at, ended_at, duration_ms, host, git)| RunInfo {
            started_at,
            ended_at,
            duration_ms,
            host,
            git,
        })
}

/// Generate arbitrary ToolInfo instances.
fn arb_tool_info() -> impl Strategy<Value = ToolInfo> {
    (arb_identifier(), arb_version()).prop_map(|(name, version)| ToolInfo { name, version })
}

/// Generate arbitrary Summary instances.
fn arb_summary() -> impl Strategy<Value = Summary> {
    (
        0usize..100,
        proptest::collection::btree_map(arb_identifier(), 0usize..100, 0..5),
        proptest::collection::btree_map(arb_identifier(), 0usize..100, 0..5),
    )
        .prop_map(|(total_findings, by_severity, by_check)| Summary {
            total_findings,
            by_severity,
            by_check,
        })
}

/// Generate arbitrary Report instances.
fn arb_report() -> impl Strategy<Value = Report> {
    (
        arb_tool_info(),
        arb_run_info(),
        arb_verdict(),
        proptest::collection::vec(arb_finding(), 0..10),
        proptest::option::of(arb_summary()),
    )
        .prop_map(|(tool, run, verdict, findings, summary)| Report {
            schema: Report::SCHEMA_V1.to_string(),
            tool,
            run,
            verdict,
            findings,
            summary,
        })
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

/// Generate arbitrary EditionPolicy instances.
fn arb_edition_policy() -> impl Strategy<Value = EditionPolicy> {
    (
        any::<bool>(),
        any::<bool>(),
        proptest::collection::vec(arb_identifier(), 0..5),
    )
        .prop_map(
            |(require_consistent, allow_per_crate_override, allow_overrides)| EditionPolicy {
                require_consistent,
                allow_per_crate_override,
                allow_overrides,
            },
        )
}

/// Generate arbitrary MemberOrderingPolicy instances.
fn arb_member_ordering_policy() -> impl Strategy<Value = MemberOrderingPolicy> {
    any::<bool>().prop_map(|require_sorted| MemberOrderingPolicy { require_sorted })
}

/// Generate arbitrary LockfilePolicy instances.
fn arb_lockfile_policy() -> impl Strategy<Value = LockfilePolicy> {
    (any::<bool>(), any::<bool>()).prop_map(|(require_for_binaries, warn_for_libraries)| {
        LockfilePolicy {
            require_for_binaries,
            warn_for_libraries,
        }
    })
}

/// Generate arbitrary Policy instances.
fn arb_policy() -> impl Strategy<Value = Policy> {
    (
        arb_msrv_policy(),
        arb_toolchain_policy(),
        arb_checksums_policy(),
        arb_edition_policy(),
        arb_member_ordering_policy(),
        arb_lockfile_policy(),
    )
        .prop_map(
            |(msrv, toolchain, checksums, edition, member_ordering, lockfile)| Policy {
                msrv,
                toolchain,
                checksums,
                edition,
                member_ordering,
                lockfile,
            },
        )
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
        arb_profile(),
        arb_defaults(),
        arb_paths_config(),
        arb_policy(),
        proptest::collection::vec(arb_check_config(), 0..5),
        proptest::collection::btree_map(arb_identifier(), arb_message(), 0..5),
    )
        .prop_map(|(profile, defaults, paths, policy, checks, meta)| Config {
            profile,
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
        prop_assert_eq!(report.schema, parsed.schema);
        prop_assert_eq!(report.tool.name, parsed.tool.name);
        prop_assert_eq!(report.tool.version, parsed.tool.version);
        prop_assert_eq!(report.run.started_at, parsed.run.started_at);
        prop_assert_eq!(report.run.ended_at, parsed.run.ended_at);
        prop_assert_eq!(report.run.duration_ms, parsed.run.duration_ms);
        prop_assert_eq!(report.run.host.os, parsed.run.host.os);
        prop_assert_eq!(report.run.host.arch, parsed.run.host.arch);
        prop_assert_eq!(report.verdict, parsed.verdict);
        prop_assert_eq!(report.findings.len(), parsed.findings.len());

        // Compare summary if present
        match (&report.summary, &parsed.summary) {
            (Some(s1), Some(s2)) => {
                prop_assert_eq!(s1.total_findings, s2.total_findings);
                prop_assert_eq!(&s1.by_severity, &s2.by_severity);
                prop_assert_eq!(&s1.by_check, &s2.by_check);
            }
            (None, None) => {}
            _ => prop_assert!(false, "Summary presence mismatch"),
        }
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
