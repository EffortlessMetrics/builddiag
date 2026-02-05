# builddiag — Design Notes

This file is the "why" behind the architecture: the constraints we defend so builddiag stays small and trustworthy.

> See also: Each crate has its own `CLAUDE.md` with implementation rationale.

## Design Constraints

- Repo truth only; no machine assumptions.
- Deterministic output; receipts are API.
- Profiles are an adoption valve, not "org lock-in".
- Codes are stable; deprecate, don't rename.

## Finding Model: check_id + code

builddiag uses:
- `check_id` to identify the producing check
- `code` to classify the specific issue

This makes:
- Explainability clean
- Dedupe and highlighting stable
- Future actuators feasible (buildfix can safely target fixes)

### Check ID Convention

Check IDs follow the pattern `module.check_name`:
- `rust.msrv_defined`
- `workspace.resolver_v2`
- `tools.checksums_format`
- `deps.wildcard_version`

### Finding Code Convention

Codes are snake_case, globally unique:
- `missing_msrv`
- `resolver_not_v2`
- `invalid_hash`

## Workspace Discovery

Prefer purely file-based discovery:
- Parse root `Cargo.toml`
- If `[workspace]` exists, discover member manifests
- Normalize paths to repo-relative `/`

Only rely on Cargo tooling if:
- Explicitly enabled
- Treated as an adapter dependency (record versions, avoid network)

## Toolchain Interpretation

When toolchain files exist:
- Parse and validate them
- Treat "missing toolchain file" as absence of input, not an error

Pinning checks should not force policy across strangers; that's why they're profile-driven.

## Effective Config Resolution

The `effective_check_config()` function provides a single point of resolution:

```rust
pub fn effective_check_config(config: &Config, check_id: &str) -> EffectiveCheckConfig {
    // 1. Start with profile defaults
    let profile_state = config.profile.check_state(check_id);

    // 2. Apply user overrides if present
    let user_override = config.checks.iter().find(|c| c.id == check_id);

    // 3. Return final enabled state and severity
}
```

This eliminates scattered `if profile == ...` branching throughout checks.

## Summary Aggregation

The receipt summary should be:
- Stable
- Minimal
- Useful for cockpit ingestion

A good minimum:
- `total_findings`
- `by_severity` (map: info, warn, error → count)
- `by_check` (map: check_id → count)

## Receipt Shape

Keep the receipt minimal and stable:
- Required fields: `schema`, `verdict`, `findings`
- Optional fields: `tool`, `run`, `summary`, `data`
- Single extension point: report-level `data`

## Rendering Strategy

Rendering is a separate layer:
- Takes a receipt + render options
- Produces markdown and/or annotations deterministically
- Enforces budgets and stable ordering

Do not re-run checks in renderers.
Do not read repo state in renderers.

## Explain Registry

Explain entries are part of the contract. They should be:
- Test-enforced (every emitted code has an entry)
- Stable over time
- Linkable (optional URLs)

The `CHECK_DOCS` static registry provides:
- Check ID
- Human-readable name
- Description
- Help text for remediation
- Optional documentation URL
- List of codes this check can produce

## Depguard Integration

Dependency hygiene checks (`deps.*`) integrate the depguard library (in-workspace crate):
- `deps.wildcard_version` — no `*` versions
- `deps.path_missing_version` — path deps need versions for publishing
- `deps.workspace_inheritance` — suggests using workspace deps when available
- `deps.lockfile_present` — Cargo.lock required for binary crates

This keeps builddiag focused on contract validation while delegating manifest hygiene to a purpose-built library that can also be used independently.

## Profile Philosophy

### oss Profile
Safe for strangers:
- Missing convention files → skip, not fail
- Malformed present files → warn/fail as real signal
- Low friction for adoption

### team Profile
Practical gating:
- Stronger defaults for disciplined repos
- Most checks at warn level
- tools.* enabled

### strict Profile
Release discipline:
- All checks at error severity
- No silent skips
- Full enforcement

## Stability Guarantees

### Stable
- Check IDs
- Finding codes
- Exit code semantics
- Report schema (versioned)

### Unstable (may change)
- Finding message wording
- Summary aggregation details
- Render output formatting
