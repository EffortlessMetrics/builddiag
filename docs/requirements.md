# builddiag — Requirements

## Purpose

builddiag is the **repo-truth workspace contract sensor**. It validates what the repo *declares* about its Rust workspace/build contract and emits a **stable receipt** for cockpit ingestion.

> See also: Each crate has its own `CLAUDE.md` documenting implementation details.

It answers:
> "Is this repository's Rust workspace contract coherent and review-safe before we spend CI minutes compiling?"

## Truth Layer

**Repo truth**.
- Deterministic
- Offline by default
- Reads repo files only
- No builds/tests/benchmarks

## Non-goals (Hard Boundaries)

builddiag must NOT:
- Validate machine state (PATH, installed tools, local binary hashes) — that's machine truth (env-check)
- Validate dynamic truth (does it compile on MSRV, API semver correctness) — that's build truth lanes
- Own complex dependency analysis — depguard is integrated as a library for basic hygiene checks
- Write fixes — that's actuation (buildfix)

## Inputs

**Required** (meaningful operation):
- Repo root
- `Cargo.toml` (root; workspace or single-crate)

**Optional** (only if present; best-effort):
- Member manifests (workspace members)
- `rust-toolchain.toml` / `rust-toolchain`
- Tool manifests/checksums files (repo conventions; profile-gated)

## Outputs (Canonical Artifacts)

MUST write:
- `artifacts/builddiag/report.json` (schema `builddiag.report.v1`)

SHOULD write when requested:
- `artifacts/builddiag/comment.md` (PR-facing summary)

MAY emit when requested:
- GitHub Actions annotations (budgeted)

## Receipt Contract

- Schema id: `builddiag.report.v1`
- Findings are flat (not nested "checks[]"), identified by:
  - `check_id` (producer)
  - `code` (classification)
- Location is best-effort:
  - `location.path` strongly preferred (repo-relative, forward slashes)
  - `line/col` optional

## Verdict Semantics

`verdict` is a stable, cockpit-friendly summary:
- `pass | warn | fail | skip | error`

Tool/runtime failures:
- Process exit code `1`
- Emit a receipt if possible with `verdict="error"` and reason `tool_error`
- Include a canonical finding `tool.runtime_error`

## Exit Codes

- `0` ok (pass or warn unless warn-as-fail is enabled by config)
- `2` policy fail (blocking findings)
- `1` tool/runtime error (I/O, parse failure, invalid config, etc.)

## Profiles (Adoption Valve)

Profiles set defaults for enablement + severities:
- `oss` (safe for strangers; never fails on missing conventions)
- `team` (reasonable gating)
- `strict` (release discipline; tighter)

Profiles are applied via an "effective config" mapping step (no scattered `if profile == ...` throughout checks).

## Explainability

builddiag provides:
- `builddiag explain <check_id|code>`
- A registry that covers every emitted (check_id, code)
- CI enforcement: "every emitted code has an explain entry"

## Determinism

Given identical inputs, builddiag produces byte-stable outputs:
- Deterministic ordering of findings and rendered output
- Deterministic truncation behavior (explicitly indicated)

## Performance

Targets:
- Runs in <1s on typical workspaces
- Scales with workspace size linearly and predictably
