# Builddiag Roadmap

This document outlines the planned features and improvements for builddiag, organized by theme and priority.

## Current Status

**Latest Release:** v0.3.0 (2026-02-16)
- Core validation: MSRV, toolchain, checksums, workspace config
- Output formats: JSON, Markdown, GitHub Actions annotations
- Profile system: strict, team, oss presets
- Depguard: dependency hygiene checks
- Comprehensive test coverage and documentation

---

## Phase 1: Developer Experience (v0.3.0)

### Watch Mode
Continuous validation during development with file system watching.

- [x] Implement `builddiag watch` subcommand
- [x] Watch Cargo.toml, rust-toolchain.toml, and checksums files
- [x] Debounce rapid file changes
- [x] Clear terminal and re-display results on change
- [ ] Optional desktop notifications for status changes

### Auto-Fix Mode
Automatically fix certain issues where the correct action is unambiguous.

- [x] Implement `builddiag fix` subcommand
- [x] Auto-add missing `rust-version` to workspace Cargo.toml
- [x] Auto-update resolver to v2 in workspace
- [x] Auto-generate missing checksum entries
- [x] Dry-run mode showing proposed changes
- [x] Interactive mode for selective fixes

### Baseline and Suppressions
Allow teams to acknowledge existing findings and track new regressions.

- [x] Support `.builddiag-baseline.json` file
- [x] `builddiag baseline create` to snapshot current findings
- [x] `builddiag baseline update` to add new findings
- [x] Report only new findings vs baseline in CI
- [ ] Inline suppression comments in Cargo.toml

---

## Phase 2: Extended Checks (v0.4.0)

### License Compliance
Validate dependency license compatibility.

- [ ] Implement `license` check module
- [ ] Allow/deny list for licenses
- [ ] SPDX expression parsing
- [ ] Copyleft contamination detection
- [ ] License file presence validation

### Security Audit Integration
Bridge with cargo-audit for vulnerability checking.

- [ ] Implement `security` check module
- [ ] Integrate RustSec advisory database
- [ ] Configurable severity thresholds
- [ ] RUSTSEC ID suppressions
- [ ] Optional network mode for fresh advisories

### Dependency Freshness
Track outdated dependencies as findings.

- [ ] Implement `freshness` check module
- [ ] Configurable staleness thresholds (semver-aware)
- [ ] Major/minor/patch version policies
- [ ] Exclude patterns for intentionally pinned deps
- [ ] Integration with crates.io API

### Edition Consistency
Validate Rust edition configuration.

- [ ] Implement `edition` check module
- [ ] Workspace edition consistency
- [ ] Minimum edition requirements
- [ ] Edition migration readiness hints

---

## Phase 3: Integration and Tooling (v0.5.0)

### Pre-commit Hook
Easy Git hooks integration.

- [ ] `builddiag init-hooks` command
- [ ] Generate `.pre-commit-config.yaml` snippet
- [ ] Standalone shell hook for non-pre-commit users
- [ ] Husky integration snippet
- [ ] Quick-fail mode for faster feedback

### IDE Integration
Editor support for real-time feedback.

- [ ] VS Code extension with diagnostics
- [ ] LSP server implementation (builddiag-lsp crate)
- [ ] IntelliJ/RustRover plugin
- [ ] Neovim configuration snippets
- [ ] Problem matcher patterns for editors

### CI Platform Support
Expand beyond GitHub Actions.

- [ ] GitLab CI annotation format
- [ ] Azure DevOps annotation format
- [ ] Bitbucket Pipelines report format
- [ ] Jenkins warnings-ng plugin format
- [ ] CircleCI test results format

### SARIF Output
Static Analysis Results Interchange Format for security tools.

- [ ] Implement SARIF 2.1.0 output format
- [ ] GitHub Code Scanning integration
- [ ] Artifact upload workflow examples
- [ ] Rule metadata in SARIF schema

---

## Phase 4: Scalability (v0.6.0)

### Multi-Repository Support
Validate multiple repositories in a single run.

- [ ] `builddiag check --manifest repos.toml`
- [ ] Parallel repository processing
- [ ] Aggregated summary report
- [ ] Cross-repo policy enforcement
- [ ] Monorepo directory patterns

### Remote Configuration
Load and share configurations across teams.

- [ ] `extends: "https://..."` in config
- [ ] `extends: "file://shared/builddiag.toml"`
- [ ] Config layering and override semantics
- [ ] Caching of remote configs
- [ ] Integrity verification (checksums)

### Custom Checks Plugin System
User-defined validation rules.

- [ ] Plugin specification format
- [ ] WASM plugin runtime
- [ ] JavaScript/TypeScript plugin support
- [ ] Plugin discovery and loading
- [ ] Example plugins repository

### Trend Analysis
Track findings over time for quality dashboards.

- [ ] `builddiag history` command
- [ ] JSON-lines output for time-series data
- [ ] Finding fingerprinting for tracking
- [ ] Grafana dashboard example
- [ ] Prometheus metrics endpoint

---

## Phase 5: Enterprise Features (v1.0.0)

### SBOM Generation
Software Bill of Materials for supply chain security.

- [ ] CycloneDX format output
- [ ] SPDX format output
- [ ] VEX (Vulnerability Exploitability eXchange) support
- [ ] Integration with dependency-track
- [ ] Build provenance attestations

### Policy as Code
Declarative policy definitions.

- [ ] Rego policy engine integration
- [ ] CEL (Common Expression Language) support
- [ ] Policy testing framework
- [ ] Centralized policy repository patterns
- [ ] Compliance report generation

### Audit Logging
Enterprise compliance and audit trails.

- [ ] JSON audit log format
- [ ] Who-ran-what-when tracking
- [ ] Policy decision logging
- [ ] Integration with SIEM systems
- [ ] Signed audit records

---

## Future Ideas (Unscheduled)

These ideas are under consideration but not yet planned:

- **Cargo.lock analysis** - Validate lock file hygiene
- **Feature flag validation** - Check feature combinations compile
- **Build time estimation** - Predict CI build duration
- **Dependency graph visualization** - Interactive dep explorer
- **Migration assistant** - Help upgrade between builddiag versions
- **Self-update mechanism** - Built-in `builddiag update` command
- **Shell completions** - Bash/Zsh/Fish/PowerShell completions
- **Man page generation** - Unix manual pages
- **Homebrew formula** - macOS package distribution
- **Windows installer** - MSI/WiX installer
- **Docker image** - Official container image

---

## Contributing to the Roadmap

We welcome community input on the roadmap:

1. **Feature requests**: Open an issue with `[Feature Request]` prefix
2. **Priority feedback**: Comment on existing roadmap items
3. **Implementation interest**: Comment "I'd like to work on this" on any item
4. **Sponsorship**: Enterprise features may be prioritized with sponsorship

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

---

## Version History

| Version | Focus | Target |
|---------|-------|--------|
| 0.1.0 | Core validation | ✅ Released |
| 0.2.0 | Profiles, depguard, docs | ✅ Released |
| 0.3.0 | Developer experience | ✅ Released |
| 0.4.0 | Extended checks | Planned |
| 0.5.0 | Integration and tooling | Planned |
| 0.6.0 | Scalability | Planned |
| 1.0.0 | Enterprise features | Planned |
