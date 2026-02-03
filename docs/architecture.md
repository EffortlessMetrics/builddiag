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
- Publish the CLI crate; keep internal crates `publish=false` unless you intentionally support embedding.

**Crate purposes:**
| Crate | Responsibility |
|-------|----------------|
| `builddiag-types` | Shared types, config/report schemas, profile definitions |
| `builddiag-domain` | Pure domain logic (no I/O): version parsing, aggregation, sorting |
| `builddiag-repo` | Repository discovery: workspace members, toolchain, checksums |
| `builddiag-checks` | Check implementations and documentation registry |
| `builddiag-render` | Output rendering: Markdown, GitHub annotations |
| `builddiag-app` | Orchestration: config loading, check coordination, atomic writes |
| `builddiag-cli` | CLI: argument parsing, command routing, exit codes |
| `depguard` | Dependency hygiene library (integrated by builddiag-checks) |

## Data Flow

1. Load config (defaults + optional file + CLI overrides)
2. Build repo model from repo files (workspace discovery, toolchain file discovery, etc.)
3. Apply profile mapping → effective config
4. Run enabled checks against the repo model
5. Aggregate findings → verdict + summary
6. Write canonical artifacts:
   - `artifacts/builddiag/report.json`
   - Optional `comment.md` / annotations
7. Exit with stable code semantics

## Receipt Schema (builddiag.report.v1)

**Top-level:**
- `schema` - Schema version identifier ("builddiag.report.v1")
- `tool` - Tool info (name, version)
- `run` - Execution metadata (timestamps, duration, host, git)
- `verdict` - Overall verdict (pass, warn, fail, skip, error)
- `findings[]` - Flattened list of all findings
- `summary` - Optional aggregated statistics

**Finding identity:**
- `check_id`: stable producer id (`rust.msrv_defined`, `workspace.resolver_v2`, …)
- `code`: stable classification (`missing_msrv`, `resolver_not_v2`, …)

**Location:**
- `location.path`: repo-relative, forward slashes
- `line/col` optional

**Extension points:**
- `finding.data` (optional structured hints)
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

builddiag produces:
- `artifacts/builddiag/report.json` (canonical)
- Optional `artifacts/builddiag/comment.md`

Cockpit policy decides whether builddiag is blocking; builddiag only emits observations.
