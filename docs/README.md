# builddiag Documentation

## Overview

builddiag is the **repo-truth workspace contract sensor** for Rust repositories. It validates what a repo declares about its build contract and emits stable receipts for CI/cockpit ingestion.

## Documentation Index

| Document | Description |
|----------|-------------|
| [requirements.md](requirements.md) | Purpose, truth layer, inputs/outputs, contract |
| [architecture.md](architecture.md) | Crate layout, data flow, receipt schema |
| [design.md](design.md) | Design constraints and rationale |
| [checks.md](checks.md) | All 15 checks with profiles and remediation |
| [config.md](config.md) | Configuration schema and profile mappings |
| [testing.md](testing.md) | Testing strategy and organization |
| [implementation.md](implementation.md) | Implementation phases and status |
| [integration.md](integration.md) | CI, local hooks, and pre-commit integration |

## Crate Documentation

Each crate has its own `CLAUDE.md` with implementation-level details:

| Crate | Purpose |
|-------|---------|
| [builddiag-types](../crates/builddiag-types/CLAUDE.md) | Shared types, config/report schemas, profiles |
| [builddiag-domain](../crates/builddiag-domain/CLAUDE.md) | Pure domain logic (version parsing, aggregation) |
| [builddiag-repo](../crates/builddiag-repo/CLAUDE.md) | Repository discovery and loading |
| [builddiag-checks](../crates/builddiag-checks/CLAUDE.md) | Check implementations and documentation registry |
| [builddiag-render](../crates/builddiag-render/CLAUDE.md) | Markdown and GitHub annotation rendering |
| [builddiag-app](../crates/builddiag-app/CLAUDE.md) | Orchestration, config loading, atomic writes |
| [builddiag-cli](../crates/builddiag-cli/CLAUDE.md) | CLI entry point, argument parsing |
| [depguard](../crates/depguard/CLAUDE.md) | Dependency hygiene library |

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
