# AGENTS.md

## Purpose
builddiag is the repo-truth workspace contract sensor. It runs fast, deterministically, and offline to validate what a Rust repo declares about its build contract.

## Docs
- `docs/README.md` for the doc index and architecture overview.
- `CLAUDE.md` for repo-level commands, conventions, and architecture.
- `crates/builddiag-types/CLAUDE.md` for DTOs, config, and schema rules.
- `crates/builddiag-domain/CLAUDE.md` for pure logic and verdict rules.
- `crates/builddiag-repo/CLAUDE.md` for repo discovery and path normalization.
- `crates/builddiag-checks/CLAUDE.md` for check implementations and registry rules.
- `crates/builddiag-render/CLAUDE.md` for markdown and annotations rendering.
- `crates/builddiag-app/CLAUDE.md` for orchestration and output writing.
- `crates/builddiag-watch/CLAUDE.md` for watch-loop polling/debounce behavior.
- `crates/builddiag-fix/CLAUDE.md` for deterministic auto-fix planning and apply behavior.
- `crates/builddiag-baseline/CLAUDE.md` for baseline snapshot/filtering behavior.
- `crates/builddiag-cli/CLAUDE.md` for CLI behavior and test harness.
- `crates/depguard/CLAUDE.md` for dependency hygiene library behavior.

## Quick Commands
- `cargo test --all`
- `cargo test -p builddiag --test cucumber`

## Current Priorities
- Tighten BDD to assert the receipt contract, not just exit codes.
- Fix Windows property tests by using real TempDir workspaces.
- Keep test-only deps scoped to CLI dev-dependencies.
