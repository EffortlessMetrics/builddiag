# Requirements Document

## Introduction

This document specifies the requirements for the Developer Experience feature set (v0.3.0). These features improve the day-to-day workflow for developers using builddiag, enabling continuous validation, automatic fixes, and baseline management for existing findings.

## Glossary

- **Watch_Mode**: A persistent process that monitors files and re-runs validation on changes
- **Auto_Fix**: Automated modification of configuration files to resolve certain findings
- **Baseline**: A snapshot of known findings that are acknowledged and excluded from failure criteria
- **Suppression**: An inline annotation or configuration entry that ignores a specific finding
- **Debounce**: Delaying action until a burst of events settles to avoid redundant work

## Requirements

### Requirement 1: Watch Mode

**User Story:** As a developer, I want builddiag to automatically re-run when I modify configuration files, so that I get immediate feedback without manually re-running the command.

#### Acceptance Criteria

1. WHEN `builddiag watch` is invoked, THE Watch_Mode SHALL monitor Cargo.toml, rust-toolchain.toml, and files matching the checksums glob pattern
2. WHEN a monitored file changes, THE Watch_Mode SHALL debounce for 200ms before re-running validation
3. WHEN validation completes, THE Watch_Mode SHALL clear the terminal and display fresh results
4. WHEN validation status changes (pass→fail or fail→pass), THE Watch_Mode SHALL optionally emit a desktop notification if `--notify` flag is provided
5. THE Watch_Mode SHALL gracefully handle Ctrl+C to exit cleanly
6. THE Watch_Mode SHALL support `--format` option to control output (json, markdown, pretty)

### Requirement 2: Auto-Fix Mode

**User Story:** As a developer, I want builddiag to automatically fix issues that have an unambiguous resolution, so that I can quickly bring my repository into compliance.

#### Acceptance Criteria

1. WHEN `builddiag fix` is invoked, THE Auto_Fix SHALL analyze findings and apply fixes for supported issue types
2. THE Auto_Fix SHALL support fixing: missing workspace rust-version, resolver not v2, missing checksum entries
3. WHEN `--dry-run` flag is provided, THE Auto_Fix SHALL display proposed changes without modifying files
4. WHEN `--interactive` flag is provided, THE Auto_Fix SHALL prompt for confirmation before each fix
5. THE Auto_Fix SHALL report which fixes were applied and which findings require manual intervention
6. THE Auto_Fix SHALL preserve file formatting and comments when modifying TOML files
7. IF a fix cannot be applied safely, THEN THE Auto_Fix SHALL skip it and report the reason

### Requirement 3: Baseline Management

**User Story:** As a team lead, I want to acknowledge existing findings so that CI only fails on new issues, allowing gradual adoption without blocking all PRs.

#### Acceptance Criteria

1. WHEN `builddiag baseline create` is invoked, THE system SHALL write current findings to `.builddiag-baseline.json`
2. WHEN `builddiag check` runs with a baseline present, THE system SHALL compare findings against the baseline
3. WHEN a finding matches a baseline entry, THE system SHALL mark it as "baselined" and exclude it from failure criteria
4. WHEN a new finding appears (not in baseline), THE system SHALL report it as a regression
5. WHEN `builddiag baseline update` is invoked, THE system SHALL add current findings to the existing baseline
6. THE Baseline file SHALL include finding fingerprints (check type, location, message hash) for stable matching
7. THE Baseline file SHALL support an optional expiration date after which findings are no longer suppressed

### Requirement 4: Inline Suppressions

**User Story:** As a developer, I want to suppress specific findings directly in my Cargo.toml, so that the suppression is versioned alongside the code.

#### Acceptance Criteria

1. WHEN a `# builddiag:ignore[check-name]` comment precedes a line, THE system SHALL suppress findings for that line
2. WHEN a `# builddiag:ignore[check-name] reason: ...` comment is present, THE system SHALL record the reason in reports
3. THE system SHALL support ignoring multiple checks: `# builddiag:ignore[check1,check2]`
4. WHEN `--report-suppressed` flag is provided, THE system SHALL include suppressed findings in the report with suppression metadata
5. THE suppression syntax SHALL work in both Cargo.toml and rust-toolchain.toml files

### Requirement 5: Enhanced CLI Output

**User Story:** As a developer, I want clear, actionable output that helps me understand and resolve findings quickly.

#### Acceptance Criteria

1. THE CLI output SHALL group findings by file, then by severity (errors first, then warnings, then info)
2. THE CLI output SHALL include a summary line showing counts by severity
3. WHEN `--format=pretty` (default), THE CLI output SHALL use colors and symbols for severity indicators
4. WHEN findings have auto-fix support, THE CLI output SHALL indicate this with a hint
5. THE CLI output SHALL show relative paths from the repository root, not absolute paths
6. WHEN no findings exist, THE CLI output SHALL display a success message with check count
