# builddiag — Architecture

> See also: Each crate has its own `CLAUDE.md` with detailed implementation guidance.

## Role in the Cockpit Ecosystem

builddiag is the **repo-truth workspace contract sensor**. It runs early (seconds), emits a stable receipt, and is safe to gate PRs on.

It deliberately stays narrow:
- **builddiag**: "does the repo *declare* a coherent contract?"
- **depguard**: "are manifests hygienic?" (integrated as a library)
- **diffguard**: "did this PR introduce forbidden patterns?"
- **covguard/lintdiff/perfgate**: build-truth consumers (diff-mapped)
- **env-check**: machine truth (onboarding/runner sanity)
- **buildfix**: actuation (plan/apply allowlisted edits)
- **cockpitctl**: ingest+render only

## System Boundary (Hexagonal)

**Domain** owns:
- Check semantics
- Profile mapping → effective config
- Verdict aggregation
- Deterministic ordering
- Explain registry contract

**Adapters** own:
- Filesystem reads
- (optional) git metadata collection
- CLI parsing
- Writing artifacts and render targets

## Workspace Layout (Microcrates)

A clean split that matches ports/adapters:

```
builddiag-cli      CLI entry point, argument parsing (clap)
       ↓
builddiag-core     Public library facade (Clap-free, embeddable)
       ↓
builddiag-app      Orchestration, config loading, atomic output writing
       ↓
builddiag-render   Markdown & GitHub annotation rendering (budget-aware)
builddiag-checks   Check implementations (MSRV, toolchain, checksums, deps)
       ↓           │
builddiag-repo     │ depguard (dependency hygiene library)
       ↓           │
builddiag-domain   Core logic (version parsing, summarization, sorting)
       ↓
builddiag-types    Shared types, config schema, report schema, profiles
```

Each crate has its own `CLAUDE.md` with detailed documentation.

**Publishing posture:**
- Publish the CLI crate and its dependency crates; document internal crates as implementation detail.

**Crate purposes:**
| Crate | Responsibility |
|-------|----------------|
| `builddiag-types` | Shared types, config/report schemas, profile definitions |
| `builddiag-domain` | Pure domain logic (no I/O): version parsing, aggregation, sorting |
| `builddiag-repo` | Repository discovery: workspace members, toolchain, checksums |
| `builddiag-checks` | Check implementations and documentation registry |
| `builddiag-render` | Output rendering: Markdown, GitHub annotations |
| `builddiag-app` | Orchestration: config loading, check coordination, atomic writes |
| `builddiag-core` | Public library facade: Clap-free API, substrate bridge, dual-format output |
| `builddiag-cli` | CLI: argument parsing, command routing, exit codes |
| `depguard` | Dependency hygiene library (integrated by builddiag-checks) |

## Data Flow

1. Load config (defaults + optional file + CLI overrides)
2. Build repo model from repo files (workspace discovery, toolchain file discovery, etc.)
   — or accept pre-computed `Substrate` to skip disk I/O
3. Apply profile mapping → effective config
4. Run enabled checks against the repo model
5. Aggregate findings → verdict + summary
6. Build dual reports:
   - `builddiag.report.v1` — native report
   - `sensor.report.v1` — Cockpit CI sensor envelope
7. Write canonical artifacts:
   - `artifacts/builddiag/report.json`
   - Optional `comment.md` / annotations
   - With `--artifacts-dir`: `<dir>/report.json` (sensor) + `<dir>/extras/payload.json` (native)
8. Exit with stable code semantics

## Substrate Bridge

When builddiag is used as a library (`builddiag-core`), callers can supply a `Substrate` struct
containing pre-computed repository state (manifests, toolchain info, checksums presence). This
skips disk-based repo discovery, enabling zero-I/O in-process integration.

```rust
let substrate = Substrate {
    manifests: vec![ManifestInfo { ... }],
    has_toolchain: true,
    toolchain_channel: Some("1.75.0".to_string()),
    has_checksums: false,
    has_lockfile: true,
    workspace_msrv: Some("1.75".to_string()),
};
let settings = Settings { substrate: Some(substrate), ..Default::default() };
let result = builddiag_core::run(&settings)?;
```

## Sensor Report (sensor.report.v1)

The sensor report wraps builddiag's native report in the Cockpit CI governance envelope:

- `schema`: `"sensor.report.v1"`
- `sensor`: tool identity and version
- `verdict`: structured `SensorVerdict` with status, counts, reasons, and optional data
- `capabilities`: map of capabilities for "No Green By Omission" tracking
- `findings`: extended `SensorFinding` with fingerprint (SHA-256), help, and URL
- `payload`: the full native `builddiag.report.v1` report

Contract schema lives at `contracts/schemas/sensor.report.v1.schema.json`.

## Receipt Schema (builddiag.report.v1)

**Top-level:**
- `schema` - Schema version identifier ("builddiag.report.v1")
- `verdict` - Overall verdict (pass, warn, fail, skip, error)
- `findings[]` - Flattened list of all findings
- `tool` - Tool info (name, version) (optional)
- `run` - Execution metadata (timestamps, duration, host, git) (optional)
- `summary` - Optional aggregated statistics
- `data` - Optional report-level data for downstream tooling

Required fields are `schema`, `verdict`, and `findings`.

**Finding identity:**
- `check_id`: stable producer id (`rust.msrv_defined`, `workspace.resolver_v2`, …)
- `code`: stable classification (`missing_msrv`, `resolver_not_v2`, …)

**Location:**
- `location.path`: repo-relative, forward slashes
- `line/col` optional

**Extension points:**
- Report-level `data` (optional tool-specific payload)

Director/cockpit treats these as opaque.

## Determinism (Contractual)

Stable sort key for findings:
1. Severity (error > warn > info)
2. check_id
3. location.path
4. location.line (missing last)
5. code
6. message

Renderers follow the same ordering and apply explicit budgets.

## Failure Behavior

- Missing optional inputs (e.g., no toolchain file) should not crash; it should either skip the relevant check or emit an informative warning depending on profile/config.
- Parse errors should be tool/runtime errors:
  - Exit 1
  - Emit receipt when possible with `tool.runtime_error`

## Integration with Cockpit

builddiag produces two report formats:
- `builddiag.report.v1` — native report with findings, verdict, and metadata
- `sensor.report.v1` — Cockpit CI sensor envelope wrapping the native report

Default artifact layout:
- `artifacts/builddiag/report.json` (native)
- `artifacts/builddiag/comment.md`

With `--artifacts-dir <dir>`:
- `<dir>/report.json` (sensor.report.v1)
- `<dir>/extras/payload.json` (builddiag.report.v1)

Cockpit policy decides whether builddiag is blocking; builddiag only emits observations.
