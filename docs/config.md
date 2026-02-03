# builddiag — Configuration

builddiag supports:
- Profiles (`oss|team|strict`) for adoptability
- Per-check overrides (enablement + severity)
- Deterministic defaults for artifact outputs

> See also: `builddiag-types/CLAUDE.md` for config schema implementation details.

## Configuration Sources (Precedence)

1. CLI flags
2. `.builddiag.toml` (or repo-local config path)
3. Defaults (compiled)

## Canonical Artifact Output

Default output directory:
- `artifacts/builddiag/`

Default filenames:
- `report.json`
- `comment.md` (when `--md` is enabled)

The CLI allows override, but the defaults are canonical for cockpit ingestion.

## Example: Minimal `.builddiag.toml`

```toml
profile = "oss"

[defaults]
out_dir = "artifacts/builddiag"
fail_on = "error"

[policy.msrv]
require_defined = true
source = "workspace"

[policy.toolchain]
require_pinned = true
relation_to_msrv = "equals"

[[checks]]
id = "workspace.resolver_v2"
enabled = true
severity = "warn"
```

## Profiles and Effective Config

Profiles are defaults only. The effective config is computed once:

1. Start with profile defaults
2. Apply per-check overrides from config file
3. Apply CLI overrides last

The `effective_check_config()` function provides a single point of configuration resolution.

### Profile Severity Mappings

#### `oss` (default) — permissive, never fails on missing conventions

| Check | Enabled | Severity |
|-------|---------|----------|
| rust.msrv_defined | yes | warn |
| rust.msrv_consistent | yes | error |
| rust.toolchain_pinning | yes | info |
| rust.toolchain_msrv_relation | yes | warn |
| workspace.resolver_v2 | yes | info |
| workspace.edition_consistent | yes | warn |
| workspace.member_ordering | yes | info |
| deps.wildcard_version | yes | info |
| deps.path_missing_version | yes | info |
| deps.workspace_inheritance | yes | info |
| deps.lockfile_present | yes | info |
| tools.* | **skip** | — |

#### `team` — reasonable gating for disciplined repos

| Check | Enabled | Severity |
|-------|---------|----------|
| rust.msrv_defined | yes | warn |
| rust.msrv_consistent | yes | error |
| rust.toolchain_pinning | yes | warn |
| rust.toolchain_msrv_relation | yes | error |
| workspace.resolver_v2 | yes | warn |
| workspace.edition_consistent | yes | error |
| workspace.member_ordering | yes | info |
| deps.wildcard_version | yes | warn |
| deps.path_missing_version | yes | warn |
| deps.workspace_inheritance | yes | warn |
| deps.lockfile_present | yes | warn |
| tools.checksums_file_exists | yes | warn |
| tools.checksums_format | yes | warn |
| tools.checksums_coverage | yes | warn |
| tools.checksums_verify_local | yes | warn |

#### `strict` — CI/release discipline

All checks enabled at **error** severity.

## Per-Check Config Knobs

Each check accepts these knobs:

- `id` — Check identifier (required)
- `enabled` — `true|false` (default: true)
- `severity` — `info|warn|error` (default: error)
- `triggers` — File patterns for diff-aware mode (optional)

```toml
[[checks]]
id = "rust.msrv_defined"
enabled = true
severity = "error"
triggers = ["Cargo.toml", "**/Cargo.toml"]
```

## Policy Settings

### MSRV Policy

```toml
[policy.msrv]
require_defined = true           # Require MSRV to be explicitly defined
source = "workspace"             # "workspace" or "any"
allow_per_crate_override = false # Allow individual crates to override
allow_overrides = []             # List of crate names allowed to differ
```

### Toolchain Policy

```toml
[policy.toolchain]
require_pinned = true            # Require specific version pinning
relation_to_msrv = "equals"      # "equals" or "at_least"
allow_nightly = false            # Allow nightly channel
```

### Checksums Policy

```toml
[policy.checksums]
require_file = true              # Require checksums file to exist
require_coverage = false         # Require all tools have checksums
verify_local_files = false       # Verify local files match checksums
```

### Edition Policy

```toml
[policy.edition]
require_consistent = true        # Require consistent edition across workspace
allow_per_crate_override = false # Allow individual crates to override
allow_overrides = []             # List of crate paths allowed to differ
```

### Member Ordering Policy

```toml
[policy.member_ordering]
require_sorted = true            # Require alphabetically sorted members
```

### Lockfile Policy

```toml
[policy.lockfile]
require_for_binaries = true      # Require Cargo.lock for binary crates
warn_for_libraries = false       # Warn about lockfile in library-only crates
```

## Defaults Section

```toml
[defaults]
fail_on = "error"                # "error", "warn", or "never"
out_dir = "artifacts/builddiag"  # Output directory
diff_aware = false               # Enable diff-aware mode
base = "origin/main"             # Base ref for diff-aware
head = "HEAD"                    # Head ref for diff-aware
```

## Paths Section

```toml
[paths]
cargo_root = "Cargo.toml"
rust_toolchain = "rust-toolchain.toml"
tools_checksums = "scripts/tools.sha256"
tools_manifest = "scripts/tools.toml"
```

## Diff-Aware Mode

When `diff_aware = true`:
- Checks only run if their trigger patterns match changed files
- Uses git diff between `base` and `head` refs
- Fails open outside git contexts (all checks run)

```toml
[defaults]
diff_aware = true
base = "origin/main"
head = "HEAD"
```

Per-check triggers can be overridden:

```toml
[[checks]]
id = "rust.msrv_defined"
triggers = ["Cargo.toml", "**/Cargo.toml"]
```

## Exit Behavior

Keep ecosystem-standard exit codes:
- `0` — ok (pass or warn)
- `2` — policy fail (error findings, or warn-as-fail enabled)
- `1` — tool/runtime error

Configure warn-as-fail via `fail_on`:

```toml
[defaults]
fail_on = "warn"  # Fail on warn or error findings
```
