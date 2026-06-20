# builddiag Documentation

## Overview

builddiag is the **repo-truth workspace contract sensor** for Rust repositories. It validates what a repo declares about its build contract and emits stable receipts for CI/cockpit ingestion.

## Documentation Index

| Document | Description |
|----------|-------------|
| [requirements.md](requirements.md) | Purpose, truth layer, inputs/outputs, contract |
| [architecture.md](architecture.md) | Crate layout, data flow, receipt schema |
| [design.md](design.md) | Design constraints and rationale |
| [checks.md](checks.md) | Built-in checks with profiles and remediation |
| [config.md](config.md) | Configuration schema and profile mappings |
| [testing.md](testing.md) | Testing strategy and organization |
| [implementation.md](implementation.md) | Implementation phases and status |
| [integration.md](integration.md) | CI, local hooks, and pre-commit integration |

## Crate Documentation

Crate docs include `README.md` (scope/API) and `CLAUDE.md` where available:

| Crate | Purpose | Docs |
|-------|---------|------|
| `builddiag-types` | Shared schemas and config/report types | [README](../crates/builddiag-types/README.md), [CLAUDE](../crates/builddiag-types/CLAUDE.md) |
| `builddiag-domain` | Pure logic for versions, summaries, verdicts, and fingerprints | [README](../crates/builddiag-domain/README.md), [CLAUDE](../crates/builddiag-domain/CLAUDE.md) |
| `builddiag-paths` | Path normalization helpers for repo-relative outputs | [README](../crates/builddiag-paths/README.md), [CLAUDE](../crates/builddiag-paths/CLAUDE.md) |
| `builddiag-repo` | Repository discovery, workspace parsing, and repo-state loading | [README](../crates/builddiag-repo/README.md), [CLAUDE](../crates/builddiag-repo/CLAUDE.md) |
| `builddiag-checks` | Check registry and check implementations | [README](../crates/builddiag-checks/README.md), [CLAUDE](../crates/builddiag-checks/CLAUDE.md) |
| `builddiag-render` | Markdown, annotation, and diagnostics rendering | [README](../crates/builddiag-render/README.md), [CLAUDE](../crates/builddiag-render/CLAUDE.md) |
| `builddiag-receipt` | Receipt conversion and capability contracts (`sensor.report.v1`) | [README](../crates/builddiag-receipt/README.md), [CLAUDE](../crates/builddiag-receipt/CLAUDE.md) |
| `builddiag-app` | Internal orchestration and output writing | [README](../crates/builddiag-app/README.md), [CLAUDE](../crates/builddiag-app/CLAUDE.md) |
| `builddiag-watch` | Polling watch loop utilities | [README](../crates/builddiag-watch/README.md), [CLAUDE](../crates/builddiag-watch/CLAUDE.md) |
| `builddiag-fix` | Deterministic fix planning and apply | [README](../crates/builddiag-fix/README.md), [CLAUDE](../crates/builddiag-fix/CLAUDE.md) |
| `builddiag-hooks` | Hook snippet generation for pre-commit/Git/Husky | [README](../crates/builddiag-hooks/README.md), [CLAUDE](../crates/builddiag-hooks/CLAUDE.md) |
| `builddiag-baseline` | Baseline snapshot/filtering and inline suppressions | [README](../crates/builddiag-baseline/README.md), [CLAUDE](../crates/builddiag-baseline/CLAUDE.md) |
| `builddiag-core` | Public library API facade for embedding builddiag | [README](../crates/builddiag-core/README.md) |
| `builddiag-cli` (`builddiag`) | CLI entry point and command routing | [README](../crates/builddiag-cli/README.md), [CLAUDE](../crates/builddiag-cli/CLAUDE.md) |
| `depguard` | Dependency hygiene library used by builddiag checks | [README](../crates/depguard/README.md), [CLAUDE](../crates/depguard/CLAUDE.md) |

## Quick Reference

### Commands
```bash
builddiag check --root .                    # Run all checks
builddiag check --profile strict            # Use strict profile
builddiag explain rust.msrv_defined         # Explain a check
builddiag list-checks                       # List all checks
```

### Profiles
- **oss** — Safe for open source; skips tools.* checks
- **team** — Practical gating for disciplined repos
- **strict** — All checks at error severity

### Exit Codes
- `0` — Pass or warn (unless fail_on=warn)
- `1` — Tool/runtime error
- `2` — Policy violation
