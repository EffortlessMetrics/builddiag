# Implementation Tasks

## Overview

This document lists the implementation tasks for the Developer Experience feature set, organized by component with dependencies noted.

---

## Phase 1: Foundation

### Task 1: Update builddiag-types with baseline and suppression types

Add new types to support baseline management and inline suppressions.

**Files to modify:**
- `crates/builddiag-types/src/lib.rs`
- `crates/builddiag-types/src/baseline.rs` (new)
- `crates/builddiag-types/src/suppression.rs` (new)

**Acceptance criteria:**
- [ ] `Baseline` struct with version, created_at, entries
- [ ] `BaselineEntry` struct with fingerprint, check, file, message, timestamps, reason
- [ ] `Suppression` struct with source enum and optional reason
- [ ] `SuppressionSource` enum: Baseline, Inline
- [ ] All types derive Serialize, Deserialize, JsonSchema
- [ ] JSON schema regenerated via xtask

---

### Task 2: Implement fingerprinting for findings

Add stable fingerprint generation for finding comparison.

**Files to modify:**
- `crates/builddiag-domain/src/fingerprint.rs` (new)
- `crates/builddiag-domain/src/lib.rs`

**Acceptance criteria:**
- [ ] `fingerprint(finding: &Finding) -> String` function
- [ ] Uses SHA-256 truncated to 16 hex chars
- [ ] Fingerprint based on: check type, file path, message content
- [ ] Unit tests for fingerprint stability
- [ ] Property test: fingerprint determinism across runs

---

### Task 3: Add suppression comment parsing to builddiag-repo

Parse inline suppression comments from TOML files.

**Files to modify:**
- `crates/builddiag-repo/src/suppression.rs` (new)
- `crates/builddiag-repo/src/state.rs`
- `crates/builddiag-repo/src/lib.rs`

**Acceptance criteria:**
- [ ] Parse `# builddiag:ignore[check1,check2]` syntax
- [ ] Parse optional `reason: ...` suffix
- [ ] Associate suppressions with line numbers
- [ ] Store in `RepoState.suppressions` map
- [ ] Unit tests for various suppression formats
- [ ] Handle malformed comments gracefully (warn, don't fail)

---

## Phase 2: Baseline Management

### Task 4: Implement baseline create command

Create baseline snapshot from current findings.

**Files to modify:**
- `crates/builddiag-cli/src/commands/baseline.rs` (new)
- `crates/builddiag-cli/src/main.rs`
- `crates/builddiag-app/src/baseline.rs` (new)

**Acceptance criteria:**
- [ ] `builddiag baseline create` command
- [ ] Generates fingerprints for all current findings
- [ ] Writes `.builddiag-baseline.json` with schema version
- [ ] `--output <path>` flag for custom location
- [ ] `--expires <days>` flag for expiration dates
- [ ] Integration test: create baseline, verify contents

**Depends on:** Task 1, Task 2

---

### Task 5: Implement baseline comparison in check command

Compare findings against baseline during validation.

**Files to modify:**
- `crates/builddiag-app/src/check.rs`
- `crates/builddiag-app/src/baseline.rs`
- `crates/builddiag-cli/src/commands/check.rs`

**Acceptance criteria:**
- [ ] Auto-detect `.builddiag-baseline.json` if present
- [ ] `--baseline <path>` flag to specify path
- [ ] `--no-baseline` flag to ignore baseline
- [ ] Partition findings: baselined, new, resolved
- [ ] Only fail on new findings (not baselined)
- [ ] Report resolved findings (in baseline but no longer present)
- [ ] Check expiration dates, warn on expired entries
- [ ] Integration test: baseline comparison flow

**Depends on:** Task 4

---

### Task 6: Implement baseline update and show commands

Update existing baseline and display contents.

**Files to modify:**
- `crates/builddiag-cli/src/commands/baseline.rs`
- `crates/builddiag-app/src/baseline.rs`

**Acceptance criteria:**
- [ ] `builddiag baseline update` adds new findings to existing
- [ ] `builddiag baseline show` displays baseline in readable format
- [ ] `builddiag baseline clear` removes baseline file
- [ ] Update preserves existing reasons and expiration dates
- [ ] Show command supports `--format json|table`
- [ ] Integration tests for all subcommands

**Depends on:** Task 4

---

## Phase 3: Auto-Fix Mode

### Task 7: Create builddiag-fix crate

New crate for auto-fix functionality.

**Files to create:**
- `crates/builddiag-fix/Cargo.toml`
- `crates/builddiag-fix/src/lib.rs`
- `crates/builddiag-fix/src/plan.rs`
- `crates/builddiag-fix/src/toml_editor.rs`

**Acceptance criteria:**
- [ ] Crate structure following workspace conventions
- [ ] `FixPlan` and `PlannedFix` types
- [ ] `FixAction` enum for supported fixes
- [ ] `TomlEditor` wrapper around toml_edit
- [ ] Unit tests for TOML editing (preserves comments)

---

### Task 8: Implement MSRV fix

Auto-add missing rust-version to workspace Cargo.toml.

**Files to modify:**
- `crates/builddiag-fix/src/fixes/msrv.rs` (new)
- `crates/builddiag-fix/src/fixes/mod.rs` (new)

**Acceptance criteria:**
- [ ] Detect missing `package.rust-version` in workspace
- [ ] Generate fix adding rust-version field
- [ ] Infer version from rust-toolchain.toml if present
- [ ] Preview shows exact TOML change
- [ ] Unit test: apply fix to sample Cargo.toml
- [ ] Verify formatting preserved

**Depends on:** Task 7

---

### Task 9: Implement resolver fix

Auto-update resolver to v2.

**Files to modify:**
- `crates/builddiag-fix/src/fixes/resolver.rs` (new)

**Acceptance criteria:**
- [ ] Detect resolver = "1" or missing resolver
- [ ] Generate fix setting resolver = "2"
- [ ] Handle both workspace and single-crate layouts
- [ ] Unit test: various Cargo.toml layouts

**Depends on:** Task 7

---

### Task 10: Implement checksum fix

Auto-generate missing checksum entries.

**Files to modify:**
- `crates/builddiag-fix/src/fixes/checksums.rs` (new)

**Acceptance criteria:**
- [ ] Detect missing checksum entries
- [ ] Download tool and compute checksum (with user consent)
- [ ] Generate fix adding checksum to config
- [ ] Support dry-run showing what would be downloaded
- [ ] Unit test: checksum generation

**Depends on:** Task 7

---

### Task 11: Implement fix CLI command

Wire up fix command with interactive mode.

**Files to modify:**
- `crates/builddiag-cli/src/commands/fix.rs` (new)
- `crates/builddiag-cli/src/main.rs`
- `crates/builddiag-fix/src/interactive.rs` (new)

**Acceptance criteria:**
- [ ] `builddiag fix` runs all applicable fixes
- [ ] `--dry-run` shows fixes without applying
- [ ] `--interactive` prompts for each fix
- [ ] `--checks <list>` limits to specific check types
- [ ] Report applied fixes and unfixable findings
- [ ] Integration test: end-to-end fix flow

**Depends on:** Task 8, Task 9, Task 10

---

## Phase 4: Watch Mode

### Task 12: Create builddiag-watch crate

New crate for file watching functionality.

**Files to create:**
- `crates/builddiag-watch/Cargo.toml`
- `crates/builddiag-watch/src/lib.rs`
- `crates/builddiag-watch/src/watcher.rs`
- `crates/builddiag-watch/src/debouncer.rs`

**Acceptance criteria:**
- [ ] Crate structure following workspace conventions
- [ ] Wrapper around `notify` crate
- [ ] Debouncer with configurable delay
- [ ] Unit tests for debouncing logic

---

### Task 13: Implement watch loop

Core watch loop with terminal output.

**Files to modify:**
- `crates/builddiag-watch/src/loop.rs` (new)
- `crates/builddiag-watch/src/lib.rs`

**Acceptance criteria:**
- [ ] Watch specified paths for changes
- [ ] Debounce events (200ms default)
- [ ] Clear terminal on each run
- [ ] Display results with timestamps
- [ ] Handle Ctrl+C gracefully
- [ ] Support --format flag (pretty, json, markdown)

**Depends on:** Task 12

---

### Task 14: Add desktop notifications

Optional notification on status change.

**Files to modify:**
- `crates/builddiag-watch/src/notifier.rs` (new)
- `crates/builddiag-watch/Cargo.toml`

**Acceptance criteria:**
- [ ] `notify-rust` as optional dependency
- [ ] Send notification on pass→fail or fail→pass
- [ ] `--notify` flag enables feature
- [ ] Gracefully handle notification failures
- [ ] Works on macOS, Linux, Windows

**Depends on:** Task 13

---

### Task 15: Implement watch CLI command

Wire up watch command.

**Files to modify:**
- `crates/builddiag-cli/src/commands/watch.rs` (new)
- `crates/builddiag-cli/src/main.rs`

**Acceptance criteria:**
- [ ] `builddiag watch` starts watch mode
- [ ] `--debounce-ms` configures delay
- [ ] `--notify` enables notifications
- [ ] Inherits --root, --config from check command
- [ ] Integration test: watch mode startup/shutdown

**Depends on:** Task 13, Task 14

---

## Phase 5: Enhanced Output

### Task 16: Implement grouping and sorting in CLI output

Improve default CLI output format.

**Files to modify:**
- `crates/builddiag-render/src/pretty.rs` (new or modify existing)
- `crates/builddiag-cli/src/commands/check.rs`

**Acceptance criteria:**
- [ ] Group findings by file
- [ ] Sort by severity within groups (error > warn > info)
- [ ] Summary line with counts per severity
- [ ] Colored severity indicators (if TTY)
- [ ] Indicate fixable findings with hint
- [ ] Use relative paths from repo root

---

### Task 17: Add suppression reporting

Include suppressed findings in reports when requested.

**Files to modify:**
- `crates/builddiag-app/src/check.rs`
- `crates/builddiag-render/src/markdown.rs`
- `crates/builddiag-render/src/json.rs`

**Acceptance criteria:**
- [ ] `--report-suppressed` flag includes suppressed findings
- [ ] Suppressed findings marked in JSON output
- [ ] Suppressed findings in separate Markdown section
- [ ] Include suppression source and reason
- [ ] Integration test: suppression reporting

**Depends on:** Task 3, Task 5

---

## Phase 6: Documentation and Polish

### Task 18: Update configuration schema

Add new config sections for baseline, watch, fix.

**Files to modify:**
- `crates/builddiag-types/src/config.rs`
- Regenerate JSON schemas

**Acceptance criteria:**
- [ ] `[baseline]` section with path, fail_on_new, warn_on_expired
- [ ] `[watch]` section with debounce_ms, notify
- [ ] `[fix]` section with backup, interactive
- [ ] All fields optional with sensible defaults
- [ ] Schema validation tests

---

### Task 19: Update documentation

Document new features in guides.

**Files to modify:**
- `docs/user-guide.md`
- `docs/configuration.md`
- `CHANGELOG.md`
- `README.md`

**Acceptance criteria:**
- [ ] Watch mode usage documented
- [ ] Fix mode usage documented
- [ ] Baseline management documented
- [ ] Inline suppression syntax documented
- [ ] Examples for common workflows
- [ ] CHANGELOG updated for v0.3.0

---

### Task 20: Final checkpoint

Verify all features work together.

**Acceptance criteria:**
- [ ] All tests pass: `cargo test --all`
- [ ] Clippy clean: `cargo clippy --all-targets`
- [ ] Format clean: `cargo fmt --check`
- [ ] Schemas up to date: `cargo run -p xtask -- schema`
- [ ] Integration test: full workflow (check → baseline → fix → watch)
- [ ] Manual testing of CLI commands

**Depends on:** All previous tasks

---

## Task Dependencies Graph

```
Task 1 (types) ─────────────────────────────────────────┐
    │                                                   │
    ▼                                                   │
Task 2 (fingerprint) ──────┐                            │
    │                      │                            │
    ▼                      ▼                            │
Task 3 (suppression) ─► Task 4 (baseline create)        │
                           │                            │
                           ▼                            │
                       Task 5 (baseline compare) ◄──────┤
                           │                            │
                           ▼                            │
                       Task 6 (baseline update/show)    │
                                                        │
Task 7 (fix crate) ────────────────────────────────────┤
    │                                                   │
    ├──► Task 8 (msrv fix)                              │
    ├──► Task 9 (resolver fix)                          │
    └──► Task 10 (checksum fix)                         │
              │                                         │
              ▼                                         │
         Task 11 (fix CLI)                              │
                                                        │
Task 12 (watch crate) ──────────────────────────────────┤
    │                                                   │
    ▼                                                   │
Task 13 (watch loop)                                    │
    │                                                   │
    ▼                                                   │
Task 14 (notifications)                                 │
    │                                                   │
    ▼                                                   │
Task 15 (watch CLI)                                     │
                                                        │
Task 16 (enhanced output) ◄─────────────────────────────┤
    │                                                   │
    ▼                                                   │
Task 17 (suppression reporting) ◄── Task 3, Task 5      │
                                                        │
Task 18 (config schema) ◄───────────────────────────────┘
    │
    ▼
Task 19 (docs)
    │
    ▼
Task 20 (final checkpoint)
```
