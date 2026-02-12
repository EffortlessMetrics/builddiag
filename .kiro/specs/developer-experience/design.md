# Design Document

## Overview

This document describes the technical design for the Developer Experience feature set. The implementation adds three new CLI subcommands (`watch`, `fix`, `baseline`) and enhances the existing `check` command with baseline comparison and suppression support.

## Architecture

### New Crate: builddiag-watch

A new crate handles file watching functionality:

```
builddiag-watch
├── src/
│   ├── lib.rs          # Public API
│   ├── watcher.rs      # File system watcher wrapper
│   ├── debouncer.rs    # Event debouncing logic
│   └── notifier.rs     # Desktop notification support
```

**Dependencies:**
- `notify` - Cross-platform file system notifications
- `notify-debouncer-mini` - Debouncing wrapper
- `notify-rust` (optional) - Desktop notifications

### New Crate: builddiag-fix

A new crate handles auto-fix functionality:

```
builddiag-fix
├── src/
│   ├── lib.rs          # Public API and fix orchestration
│   ├── toml_edit.rs    # TOML modification preserving formatting
│   ├── fixes/
│   │   ├── mod.rs
│   │   ├── msrv.rs     # Add missing rust-version
│   │   ├── resolver.rs # Update to resolver v2
│   │   └── checksums.rs # Generate missing checksums
│   └── interactive.rs  # Interactive prompting
```

**Dependencies:**
- `toml_edit` - Format-preserving TOML editing
- `dialoguer` - Interactive prompts

### Updated Crates

**builddiag-types:**
```rust
/// Baseline entry for a single finding
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct BaselineEntry {
    pub fingerprint: String,
    pub check: String,
    pub file: Utf8PathBuf,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
}

/// Baseline file structure
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Baseline {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub entries: Vec<BaselineEntry>,
}

/// Suppression metadata attached to findings
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Suppression {
    pub source: SuppressionSource,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum SuppressionSource {
    Baseline,
    Inline { line: u32 },
}
```

**builddiag-app:**
- Add baseline loading and comparison logic
- Add suppression parsing from TOML comments
- Integrate fix suggestions into findings

## Component Design

### Watch Mode

```
┌─────────────────────────────────────────────────────────┐
│                    Watch Loop                           │
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐    ┌──────────┐    ┌─────────────┐        │
│  │ Watcher │───▶│ Debouncer│───▶│ Validator   │        │
│  └─────────┘    └──────────┘    └─────────────┘        │
│       │              │                  │               │
│       │         200ms delay             │               │
│       │                                 ▼               │
│       │                          ┌───────────┐         │
│       │                          │ Renderer  │         │
│       │                          └───────────┘         │
│       │                                 │               │
│       │                                 ▼               │
│       │    status change?        ┌───────────┐         │
│       │◀─────────────────────────│ Notifier  │         │
│       │                          └───────────┘         │
└───────┴─────────────────────────────────────────────────┘
```

**Watched Paths:**
1. `Cargo.toml` (workspace and all members)
2. `rust-toolchain.toml` or `rust-toolchain`
3. Files matching checksums glob pattern from config
4. `.builddiag.toml` (config file itself)

**Debouncing Strategy:**
- 200ms debounce window (configurable via `--debounce-ms`)
- Coalesce all events within window into single validation run
- Reset timer on each new event

### Auto-Fix Mode

```
┌─────────────────────────────────────────────────────────┐
│                    Fix Pipeline                         │
├─────────────────────────────────────────────────────────┤
│  ┌─────────┐    ┌──────────┐    ┌─────────────┐        │
│  │ Analyze │───▶│ Plan     │───▶│ Apply       │        │
│  └─────────┘    └──────────┘    └─────────────┘        │
│       │              │                  │               │
│       │              │                  │               │
│       ▼              ▼                  ▼               │
│  Run checks     Generate        Write files             │
│  Collect        fix plan        (if not dry-run)       │
│  findings       per finding                             │
│                      │                                  │
│                      ▼                                  │
│               ┌─────────────┐                          │
│               │ Interactive │ (if --interactive)       │
│               │ Confirm     │                          │
│               └─────────────┘                          │
└─────────────────────────────────────────────────────────┘
```

**Fix Plan Structure:**
```rust
pub struct FixPlan {
    pub fixes: Vec<PlannedFix>,
    pub unfixable: Vec<UnfixableFinding>,
}

pub struct PlannedFix {
    pub finding: Finding,
    pub action: FixAction,
    pub preview: String,
}

pub enum FixAction {
    AddWorkspaceMsrv { version: String },
    SetResolverV2,
    AddChecksumEntry { tool: String, checksum: String },
}
```

**Format Preservation:**
Using `toml_edit` instead of `toml` ensures:
- Comments are preserved
- Whitespace and formatting maintained
- Only targeted modifications applied

### Baseline Management

**Fingerprinting Algorithm:**
```rust
fn fingerprint(finding: &Finding) -> String {
    let mut hasher = Sha256::new();
    hasher.update(finding.check.as_bytes());
    hasher.update(finding.file.as_str().as_bytes());
    hasher.update(finding.message.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}
```

**Baseline Comparison Flow:**
```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Load         │───▶│ Run          │───▶│ Compare      │
│ Baseline     │    │ Checks       │    │ Fingerprints │
└──────────────┘    └──────────────┘    └──────────────┘
       │                   │                    │
       │                   │                    ▼
       │                   │           ┌──────────────┐
       │                   │           │ Partition:   │
       │                   │           │ - Baselined  │
       │                   │           │ - New        │
       │                   │           │ - Resolved   │
       │                   │           └──────────────┘
       │                   │                    │
       │                   │                    ▼
       │                   │           ┌──────────────┐
       │                   │           │ Report       │
       │                   │           │ (fail on new)│
       └───────────────────┴───────────┴──────────────┘
```

**Baseline File Location:**
- Default: `.builddiag-baseline.json` in repository root
- Override: `--baseline <path>` or config `baseline.path`

### Inline Suppression Parsing

**Syntax Grammar:**
```
suppression   = "# builddiag:ignore[" check-list "]" [reason]
check-list    = check-name ("," check-name)*
check-name    = identifier
reason        = "reason:" text
```

**Examples:**
```toml
# builddiag:ignore[msrv-workspace]
[package]
name = "legacy-crate"

# builddiag:ignore[resolver-v2] reason: waiting for MSRV bump
resolver = "1"

# builddiag:ignore[checksum-missing,checksum-mismatch]
[workspace.metadata.tools]
```

**Implementation:**
Extend TOML parsing in `builddiag-repo` to extract comments and associate them with subsequent nodes. Store suppressions in `RepoState`:

```rust
pub struct RepoState {
    // ... existing fields ...
    pub suppressions: BTreeMap<Utf8PathBuf, Vec<InlineSuppression>>,
}

pub struct InlineSuppression {
    pub line: u32,
    pub checks: Vec<String>,
    pub reason: Option<String>,
}
```

## CLI Interface

### New Subcommands

```
builddiag watch [OPTIONS]
    --root <PATH>        Repository root (default: .)
    --config <PATH>      Config file path
    --format <FORMAT>    Output format: pretty, json, markdown
    --notify             Enable desktop notifications
    --debounce-ms <MS>   Debounce delay (default: 200)

builddiag fix [OPTIONS]
    --root <PATH>        Repository root (default: .)
    --config <PATH>      Config file path
    --dry-run            Show fixes without applying
    --interactive        Prompt before each fix
    --checks <LIST>      Only fix specific check types

builddiag baseline <COMMAND>
    create               Create baseline from current findings
    update               Add new findings to existing baseline
    show                 Display baseline contents
    clear                Remove baseline file

builddiag baseline create [OPTIONS]
    --root <PATH>        Repository root (default: .)
    --output <PATH>      Baseline file path (default: .builddiag-baseline.json)
    --expires <DAYS>     Set expiration for all entries
```

### Enhanced Check Command

```
builddiag check [OPTIONS]
    --baseline <PATH>    Use baseline file (default: auto-detect)
    --no-baseline        Ignore baseline even if present
    --report-suppressed  Include suppressed findings in report
    --fail-on <LEVEL>    Exit non-zero threshold: error, warn, info
```

## Error Handling

### Watch Mode Errors

| Error | Handling |
|-------|----------|
| Watched file deleted | Remove from watch, continue |
| Permission denied | Log warning, continue watching other files |
| Too many watchers | Fall back to polling mode with warning |
| Notification failure | Log warning, continue without notifications |

### Fix Mode Errors

| Error | Handling |
|-------|----------|
| File not writable | Skip fix, report in unfixable list |
| Parse error | Skip fix, report parse error |
| Fix conflicts | Skip fix, explain conflict |
| Backup failure | Abort all fixes, restore originals |

## Testing Strategy

### Unit Tests

- Fingerprint generation determinism
- Suppression comment parsing
- Fix plan generation
- Debouncer timing behavior

### Integration Tests

- Watch mode file change detection
- Fix mode TOML modifications
- Baseline create/update/compare cycle
- Suppression filtering in reports

### Property Tests

- Fingerprints are stable across serialization round-trips
- Baseline comparison is symmetric
- Fix operations are idempotent

## Migration and Compatibility

### Config Schema Update

Add new optional sections to `.builddiag.toml`:

```toml
[baseline]
path = ".builddiag-baseline.json"
fail_on_new = true
warn_on_expired = true

[watch]
debounce_ms = 200
notify = false

[fix]
backup = true
interactive = false
```

### Report Schema Update

Add optional fields to findings:

```json
{
  "findings": [
    {
      "check": "msrv-workspace",
      "severity": "error",
      "message": "...",
      "suppression": {
        "source": "baseline",
        "reason": "tracked in JIRA-123"
      },
      "fixable": true
    }
  ]
}
```
