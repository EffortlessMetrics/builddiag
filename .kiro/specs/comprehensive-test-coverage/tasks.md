# Implementation Plan: Comprehensive Test Coverage

## Overview

This implementation plan builds out comprehensive test coverage for the builddiag Rust CLI tool. Tasks are organized to first verify the baseline (fix any failures), then systematically add each testing layer: unit tests, property tests, integration tests, fuzz tests, mutation tests, and CI integration.

## Tasks

- [ ] 1. Verify baseline and fix any preexisting test failures
  - Run `cargo test --all` and verify all tests pass
  - Fix any flaky or failing tests
  - Ensure deterministic test execution
  - _Requirements: 1.1, 1.2, 1.3_

- [ ] 2. Add property tests for builddiag-types
  - [ ] 2.1 Create types_properties.rs test file
    - Add proptest dependency to builddiag-types dev-dependencies
    - Create `crates/builddiag-types/tests/types_properties.rs`
    - _Requirements: 3.8, 3.9_
  
  - [ ] 2.2 Write property test for Config round-trip
    - **Property 1: Config Serialization Round-Trip**
    - **Validates: Requirements 3.8**
  
  - [ ] 2.3 Write property test for Report round-trip
    - **Property 2: Report Serialization Round-Trip**
    - **Validates: Requirements 3.9, 8.5**

- [ ] 3. Add property tests for builddiag-domain
  - [ ] 3.1 Create domain_properties.rs test file
    - Create `crates/builddiag-domain/tests/domain_properties.rs`
    - Add proptest to dev-dependencies
    - _Requirements: 3.1, 3.2, 3.3_
  
  - [ ] 3.2 Write property test for version parsing normalization
    - **Property 8: Version Parsing Normalization**
    - **Validates: Requirements 3.1**
  
  - [ ] 3.3 Write property test for check status consistency
    - **Property 6: Check Status Consistency**
    - **Validates: Requirements 3.2**
  
  - [ ] 3.4 Write property test for summary aggregation
    - **Property 7: Summary Aggregation Consistency**
    - **Validates: Requirements 3.3**

- [ ] 4. Checkpoint - Verify property tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Add property tests for builddiag-render
  - [ ] 5.1 Create render_properties.rs test file
    - Create `crates/builddiag-render/tests/render_properties.rs`
    - Add proptest and chrono to dev-dependencies
    - _Requirements: 3.5, 3.6_
  
  - [ ] 5.2 Write property test for deterministic output
    - **Property 5: Deterministic Output Ordering**
    - **Validates: Requirements 8.8**
  
  - [ ] 5.3 Write property test for markdown consistency
    - Test that markdown output contains expected sections for any report
    - _Requirements: 3.5_

- [ ] 6. Extend property tests for builddiag-checks
  - [ ] 6.1 Extend check_properties.rs with additional properties
    - Add tests for graceful error handling
    - Add tests for error message context
    - _Requirements: 3.4, 8.2, 8.3, 8.7_
  
  - [ ] 6.2 Write property test for graceful error handling
    - **Property 3: Graceful Error Handling**
    - **Validates: Requirements 8.2, 8.3**
  
  - [ ] 6.3 Write property test for error message context
    - **Property 4: Error Messages Contain Context**
    - **Validates: Requirements 8.7**

- [ ] 7. Add unit tests for builddiag-types
  - [ ] 7.1 Add unit tests for Config defaults
    - Test default values for all config fields
    - Test check_overrides() method
    - _Requirements: 2.1_
  
  - [ ] 7.2 Add unit tests for type constructors and methods
    - Test Finding, CheckReport, Summary construction
    - _Requirements: 2.1_

- [ ] 8. Add unit tests for builddiag-app
  - [ ] 8.1 Add unit tests for config loading
    - Test load_config with valid TOML
    - Test load_config with missing file
    - Test load_config with invalid TOML
    - _Requirements: 2.6, 8.2, 8.3_
  
  - [ ] 8.2 Add unit tests for write_atomic
    - Test atomic file writing
    - Test directory creation
    - _Requirements: 2.6_

- [ ] 9. Checkpoint - Verify all unit and property tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 10. Add CLI integration tests
  - [ ] 10.1 Create cli_check.rs integration test file
    - Test `builddiag check` with various scenarios
    - Test all policy configurations
    - _Requirements: 6.1, 7.1, 7.2, 7.3, 7.4_
  
  - [ ] 10.2 Create cli_exit_codes.rs integration test file
    - Test exit code 0 (pass)
    - Test exit code 2 (fail)
    - Test exit code 3 (warn with fail_on=warn)
    - _Requirements: 6.8_
  
  - [ ] 10.3 Create cli_md.rs integration test file
    - Test `builddiag md` command
    - Test markdown output to file and stdout
    - _Requirements: 6.2_
  
  - [ ] 10.4 Create cli_annotations.rs integration test file
    - Test `builddiag github-annotations` command
    - Test annotation format
    - _Requirements: 6.3_
  
  - [ ] 10.5 Create cli_diff_aware.rs integration test file
    - Test diff-aware mode with changed files
    - Test --always flag behavior
    - _Requirements: 6.4, 7.7_
  
  - [ ] 10.6 Create cli_config.rs integration test file
    - Test --config flag with custom config file
    - Test config file loading and override behavior
    - _Requirements: 6.5, 7.6_

- [ ] 11. Checkpoint - Verify all integration tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 12. Set up fuzz testing infrastructure
  - [ ] 12.1 Create fuzz directory structure
    - Create `fuzz/Cargo.toml` with libfuzzer-sys dependency
    - Create `fuzz/fuzz_targets/` directory
    - _Requirements: 5.1, 5.2, 5.3, 5.4_
  
  - [ ] 12.2 Create fuzz target for version parsing
    - Create `fuzz/fuzz_targets/fuzz_version.rs`
    - Fuzz parse_rust_version function
    - _Requirements: 5.3_
  
  - [ ] 12.3 Create fuzz target for TOML parsing
    - Create `fuzz/fuzz_targets/fuzz_toml.rs`
    - Fuzz rust-toolchain.toml parsing
    - _Requirements: 5.1_
  
  - [ ] 12.4 Create fuzz target for checksums parsing
    - Create `fuzz/fuzz_targets/fuzz_checksums.rs`
    - Fuzz checksums file parsing
    - _Requirements: 5.2_
  
  - [ ] 12.5 Create fuzz target for config parsing
    - Create `fuzz/fuzz_targets/fuzz_config.rs`
    - Fuzz Config TOML parsing
    - _Requirements: 5.4_

- [ ] 13. Set up mutation testing
  - [ ] 13.1 Add cargo-mutants configuration
    - Create `.cargo/mutants.toml` with exclusions
    - Configure timeout and thread settings
    - _Requirements: 4.1, 4.5_
  
  - [ ] 13.2 Run initial mutation testing baseline
    - Run `cargo mutants` and document baseline score
    - Identify any surviving mutants that need additional tests
    - _Requirements: 4.2_

- [ ] 14. Set up coverage reporting
  - [ ] 14.1 Add cargo-llvm-cov configuration
    - Add llvm-cov to workspace dev-dependencies
    - Create coverage script in xtask
    - _Requirements: 10.1, 10.5, 10.6_
  
  - [ ] 14.2 Generate initial coverage report
    - Run coverage and document baseline
    - Identify uncovered code paths
    - _Requirements: 10.6_

- [ ] 15. Update CI pipeline
  - [ ] 15.1 Update ci.yml with comprehensive test jobs
    - Add coverage job with codecov upload
    - Add mutation testing job (scheduled)
    - Add fuzz testing job (scheduled)
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 9.7, 9.8_
  
  - [ ] 15.2 Add coverage threshold enforcement
    - Configure codecov.yml with 80% threshold
    - Add coverage badge to README
    - _Requirements: 10.3, 10.4_

- [ ] 16. Add test documentation
  - [ ] 16.1 Create TESTING.md documentation
    - Document how to run each test type
    - Document test organization conventions
    - Document coverage requirements
    - _Requirements: 11.7_
  
  - [ ] 16.2 Add requirement references to existing tests
    - Add comments linking tests to requirements
    - Ensure consistent naming conventions
    - _Requirements: 11.5, 11.6_

- [ ] 17. Final checkpoint - Comprehensive test verification
  - Run full test suite: `cargo test --all`
  - Run clippy: `cargo clippy --all-targets`
  - Generate coverage report
  - Verify all requirements are covered
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- All tasks are required for comprehensive test coverage
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- Fuzz testing and mutation testing are set up but run on schedule to avoid slowing CI
