# Requirements Document

## Introduction

builddiag emits two machine-readable receipts: a `builddiag.report.v1` report and a
`sensor.report.v1` Cockpit CI sensor envelope. The behavior-driven (BDD/cucumber) suite under
`crates/builddiag-cli/tests/features/*.feature` is the CLI-level contract guard for these receipts.

Today, most feature files (`msrv_validation`, `toolchain_pinning`, `exit_codes`,
`extended_checks`, `checksums_validation`, `configuration`, `path_normalization`) assert only the
process exit code and the top-level report verdict. They do not verify the rest of the receipt
payload: individual findings by check id, code, and severity; the `sensor.report.v1` envelope
fields; capability states; verdict reasons and verdict data; artifact entries; or schema
identifiers. `receipt_contract.feature` is the existing model for thorough assertions and is the
target standard for the rest of the suite.

This feature systematically tightens the BDD suite so every scenario that runs the CLI asserts the
relevant parts of the receipt contract, not just the exit code and the top-level verdict. It also
extends the reusable step library with any missing, expressive, DRY step definitions required to
make those assertions readable and shared across feature files. The scope is limited to the BDD
feature files and their cucumber step definitions, plus the supporting test harness; it does not
change builddiag's runtime behavior or the receipt schemas themselves.

## Glossary

- **BDD_Suite**: The cucumber-based behavior test suite for the builddiag CLI, located at `crates/builddiag-cli/tests/features/*.feature`, invoked via `cargo test -p builddiag --test cucumber`.
- **Step_Library**: The collection of reusable Given/When/Then cucumber step definitions in `crates/builddiag-cli/tests/bdd/steps.rs` that scenarios compose.
- **Check_Scenario**: Any BDD scenario that executes the `builddiag check` command.
- **Report**: A JSON receipt that conforms to the `builddiag.report.v1` schema, written to the canonical report path.
- **Sensor_Report**: A JSON receipt that conforms to the `sensor.report.v1` schema, produced when the output format is `sensor` or the check mode is `cockpit`.
- **Canonical_Report_Path**: The resolved on-disk location of the receipt for a scenario, derived from the artifacts directory, explicit `--out`, configured `out_dir`, or the default `artifacts/builddiag/report.json`.
- **Finding**: A single receipt entry identified by its `check_id`, `code`, and `severity`.
- **Verdict_Reason**: A machine-addressable token in the sensor verdict `reasons` list (for example `checks_failed`, `checks_warned`).
- **Verdict_Data**: The structured `verdict.data` object in a Sensor_Report whose array fields (for example `failed_checks`, `warned_checks`) name the check ids behind each Verdict_Reason.
- **Capability_State**: The `status` (`available`, `unavailable`, or `skipped`) and optional `reason` recorded for a named capability (`git`, `config`, `toolchain`, `checksums`, `diff_aware`) in a Sensor_Report run.
- **Artifact_Entry**: An entry in the Sensor_Report `artifacts` list identified by its `name` and `path`.
- **Schema_Identifier**: The `schema` string field of a receipt, either `builddiag.report.v1` or `sensor.report.v1`.
- **Receipt_Contract_Element**: Any asserted receipt detail beyond exit code and top-level verdict, namely a Finding, Verdict_Reason, Verdict_Data entry, Capability_State, Artifact_Entry, or Schema_Identifier.

## Requirements

### Requirement 1: Finding-level assertions for check scenarios

**User Story:** As a builddiag maintainer, I want every check scenario to assert the findings it produces by check id, code, and severity, so that regressions in finding payloads are caught at the CLI contract layer.

#### Acceptance Criteria

1. WHERE a Check_Scenario expects the top-level Report verdict to indicate failure, THE BDD_Suite SHALL assert that the Report includes at least one Finding whose `check_id`, `code`, and `severity` (one of `info`, `warn`, or `error`) match the scenario's expected values.
2. WHERE a Check_Scenario expects the top-level Report verdict to indicate a pass, THE BDD_Suite SHALL assert that the Report contains zero Findings with severity `error`.
3. THE Step_Library SHALL provide a single reusable step that asserts a Finding with a specified `check_id`, `code`, and `severity` (one of `info`, `warn`, or `error`) is present in the Report.
4. THE Step_Library SHALL provide a single reusable step that asserts no Finding with a specified `check_id` and `code` is present in the Report.
5. IF a scenario asserts a Finding that is not present in the Report, THEN THE BDD_Suite SHALL fail that scenario and emit a failure message naming the expected `check_id`, `code`, and `severity`.
6. IF a scenario asserts the absence of a Finding whose `check_id` and `code` are present in the Report, THEN THE BDD_Suite SHALL fail that scenario and emit a failure message naming the `check_id` and `code` that was found.

### Requirement 2: Sensor verdict reason and data assertions

**User Story:** As a CI integrator, I want sensor scenarios to assert verdict reasons and structured verdict data, so that the downstream Cockpit governance contract is protected from drift.

#### Acceptance Criteria

1. WHERE a Check_Scenario emits a Sensor_Report, THE BDD_Suite SHALL assert that the Sensor_Report verdict status equals the single status value the scenario expects, where that value is one of `pass`, `warn`, `fail`, or `skip`.
2. WHEN a Sensor_Report verdict status is `fail` because a check failed, THE BDD_Suite SHALL assert that the sensor verdict reasons list contains the token `checks_failed`.
3. WHEN a Sensor_Report verdict status is `fail` because a check failed, THE BDD_Suite SHALL assert that the Verdict_Data array field `failed_checks` contains the failing check id.
4. WHEN a Sensor_Report verdict status is `warn` because a check warned, THE BDD_Suite SHALL assert that the sensor verdict reasons list contains the token `checks_warned`.
5. WHEN a Sensor_Report verdict status is `warn` because a check warned, THE BDD_Suite SHALL assert that the Verdict_Data array field `warned_checks` contains the warning check id.
6. THE Step_Library SHALL provide a reusable step that asserts a specified Verdict_Reason token is present in the sensor verdict reasons list.
7. THE Step_Library SHALL provide a reusable step that asserts a specified check id is present in a named Verdict_Data array field, matched independently of element ordering.
8. IF a scenario asserts a Verdict_Reason token that is absent from the sensor verdict reasons list, or asserts a check id that is absent from the named Verdict_Data array field, or the `verdict.data` object or the named array field is absent, THEN THE BDD_Suite SHALL fail the scenario with a message naming the expected token or check id and the targeted reasons list or Verdict_Data field.

### Requirement 3: Capability state assertions

**User Story:** As a downstream tooling author, I want sensor scenarios to assert capability states, so that the "No Green By Omission" guarantee remains verifiable from the receipt.

#### Acceptance Criteria

1. WHERE a Check_Scenario emits a Sensor_Report, THE BDD_Suite SHALL assert that the recorded status of each capability the scenario configures equals the scenario's expected value of `unavailable` or `skipped`.
2. THE Step_Library SHALL provide a reusable step that asserts a named capability is present in the Sensor_Report run capabilities map.
3. THE Step_Library SHALL provide a reusable step that asserts a named capability has a specified status of `available`, `unavailable`, or `skipped`.
4. THE Step_Library SHALL provide a reusable step that asserts a named capability exposes a reason that is an exact full-string match of a specified reason value.
5. IF a scenario asserts a Capability_State for a capability that is absent from the Sensor_Report run, THEN THE BDD_Suite SHALL fail the scenario with a message naming the missing capability.
6. IF a scenario asserts a status for a capability present in the Sensor_Report run and the recorded status differs from the asserted status, THEN THE BDD_Suite SHALL fail the scenario with a message naming the capability, the asserted status, and the recorded status.
7. IF a scenario asserts a reason for a capability that has no recorded reason or whose recorded reason does not match the asserted reason, THEN THE BDD_Suite SHALL fail the scenario with a message naming the capability and the asserted reason.

### Requirement 4: Artifact entry assertions

**User Story:** As a CI integrator, I want cockpit scenarios to assert each artifact entry, so that the receipt accurately references the payload and comment files downstream tools consume.

#### Acceptance Criteria

1. WHERE a Check_Scenario produces artifacts in cockpit mode, THE BDD_Suite SHALL assert that the Sensor_Report artifacts list contains each expected Artifact_Entry, matching an entry by exact, case-sensitive equality of both its name and its path.
2. WHERE a Check_Scenario produces an artifact file, THE BDD_Suite SHALL assert that a file exists on disk at the location obtained by resolving the Artifact_Entry path relative to the directory containing the Sensor_Report.
3. THE Step_Library SHALL provide a reusable step that asserts an Artifact_Entry is present in the Sensor_Report artifacts list by exact, case-sensitive match on both name and path.
4. WHEN a scenario asserts an Artifact_Entry whose name and path both exactly match a single entry in the Sensor_Report artifacts list, THE BDD_Suite SHALL pass that assertion.
5. IF a scenario asserts an Artifact_Entry whose name and path do not both exactly match any entry in the Sensor_Report artifacts list, THEN THE BDD_Suite SHALL fail the scenario with a message naming the expected artifact name and path.
6. IF an asserted Artifact_Entry is present in the Sensor_Report but no file exists on disk at its resolved path, THEN THE BDD_Suite SHALL fail the scenario with a message naming the expected artifact name and the resolved path.

### Requirement 5: Schema identifier assertions

**User Story:** As a downstream tooling author, I want every receipt-producing scenario to assert the schema identifier, so that consumers can rely on the declared contract version.

#### Acceptance Criteria

1. WHEN a Check_Scenario produces a Report, THE BDD_Suite SHALL assert the Report Schema_Identifier is an exact, case-sensitive match of the string `builddiag.report.v1`.
2. WHEN a Check_Scenario produces a Sensor_Report, THE BDD_Suite SHALL assert the Sensor_Report Schema_Identifier is an exact, case-sensitive match of the string `sensor.report.v1`.
3. WHEN a Check_Scenario produces a Sensor_Report that embeds a builddiag report payload, THE BDD_Suite SHALL assert the embedded payload Schema_Identifier is an exact, case-sensitive match of the string `builddiag.report.v1`.
4. THE Step_Library SHALL provide a reusable step that asserts the receipt at a specified path carries a specified Schema_Identifier by exact, case-sensitive string match.
5. IF a receipt or embedded payload asserted by a Schema_Identifier step is missing its schema field or carries a value other than the expected Schema_Identifier, THEN THE BDD_Suite SHALL fail the scenario with a message naming the receipt path, the expected Schema_Identifier, and the actual value found.

### Requirement 6: Reusable and DRY step library

**User Story:** As a contributor writing feature files, I want receipt assertions exposed as shared, expressive step definitions, so that scenarios stay readable and assertions are defined once.

#### Acceptance Criteria

1. WHERE a receipt assertion is used by two or more scenarios, THE Step_Library SHALL expose that assertion through exactly one step definition and SHALL contain no duplicated assertion logic for the same receipt check.
2. WHEN the Step_Library evaluates a receipt-payload assertion, THE Step_Library SHALL resolve the receipt against the Canonical_Report_Path resolved for the scenario before evaluating the assertion.
3. IF a receipt referenced by an assertion is absent at the Canonical_Report_Path, THEN THE BDD_Suite SHALL fail the scenario with a message identifying the resolved receipt path and indicating that the receipt was not found, and SHALL NOT report the assertion as passing.
4. IF a receipt referenced by an assertion fails schema validation against its declared Schema_Identifier, THEN THE BDD_Suite SHALL fail the scenario with a message identifying the resolved receipt path and the schema validation error.
5. THE Step_Library SHALL match findings, capabilities, artifacts, and verdict-data entries by their identifying fields and SHALL produce identical pass or fail results regardless of the order in which those elements appear within the receipt.

### Requirement 7: Receipt-contract coverage for every check scenario

**User Story:** As a builddiag maintainer, I want a guarantee that no check scenario stops at exit code and top-level verdict, so that the receipt contract is exercised uniformly across the suite.

#### Acceptance Criteria

1. THE BDD_Suite SHALL require every Check_Scenario to assert at least one Receipt_Contract_Element in addition to the process exit code and the top-level Report verdict.
2. WHERE a Check_Scenario expects a failing verdict, THE BDD_Suite SHALL assert the specific Finding that drives the failure identified by its check id, code, and severity.
3. WHERE a Check_Scenario emits a Sensor_Report, THE BDD_Suite SHALL assert at least one of a Verdict_Reason, a Verdict_Data entry, a Capability_State, or an Artifact_Entry in addition to the sensor verdict status.
4. IF a Check_Scenario asserts only the process exit code and the top-level Report verdict without asserting any Receipt_Contract_Element, THEN THE BDD_Suite SHALL fail that scenario with a message naming the scenario and indicating that Receipt_Contract_Element coverage is missing.

### Requirement 8: Harness compatibility and deterministic results

**User Story:** As a builddiag maintainer, I want the tightened suite to run through the existing harness and produce stable results, so that contract assertions integrate with current CI without new flakiness.

#### Acceptance Criteria

1. WHEN `cargo test -p builddiag --test cucumber` is invoked, THE BDD_Suite SHALL discover and execute every scenario defined in `crates/builddiag-cli/tests/features/*.feature` and report a pass or fail result for each scenario without harness or compilation errors.
2. WHEN the BDD_Suite is executed two or more times against identical workspace fixtures, THE BDD_Suite SHALL produce identical pass or fail results for every scenario across those executions.
3. WHERE a Receipt_Contract_Element assertion targets a receipt field whose value varies between runs with generated timestamps, host or machine identity, or absolute file-system paths, THE Step_Library SHALL evaluate that assertion such that its pass or fail outcome does not change when only those volatile values differ between runs.
4. WHERE new step definitions are added to the Step_Library, THE Step_Library SHALL keep every scenario that passed prior to the addition passing after the addition.
5. THE BDD_Suite SHALL declare every test-only dependency it introduces only within the dev-dependencies of the builddiag CLI crate.
6. THE BDD_Suite SHALL keep every test-only dependency absent from the runtime dependencies of every workspace crate.
7. WHEN the BDD_Suite executes its scenarios in differing orders against identical workspace fixtures, THE BDD_Suite SHALL produce identical pass or fail results for every scenario.
