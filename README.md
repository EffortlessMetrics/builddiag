# builddiag

[![CI](https://github.com/EffortlessMetrics/builddiag/actions/workflows/ci.yml/badge.svg)](https://github.com/EffortlessMetrics/builddiag/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/EffortlessMetrics/builddiag/branch/main/graph/badge.svg)](https://codecov.io/gh/EffortlessMetrics/builddiag)

`builddiag` checks the build contract of a Rust repository and emits:

- a versioned JSON report (machine-friendly)
- a compact Markdown summary (PR comment friendly)
- optional GitHub Actions annotations

It is designed to be fast and offline by default: it reads manifests and policy files; it does not run cargo commands.

## Install

From this repo:

```bash
cargo install --path crates/builddiag-cli
```

## Run

```bash
builddiag check
```

Artifacts default to `artifacts/builddiag/`:

- `report.json`
- `comment.md`

To emit GitHub annotations:

```bash
builddiag check --annotations github
```

### Local Usage (Copy/Paste)

```bash
builddiag check \
  --out artifacts/builddiag/report.json \
  --md artifacts/builddiag/comment.md
```

### CI Usage (GitHub Actions)

#### Reusable Workflow (Recommended)

The easiest way to integrate builddiag into your CI is using the reusable workflow:

```yaml
jobs:
  builddiag:
    uses: EffortlessMetrics/builddiag/.github/workflows/builddiag.yml@main
    with:
      profile: oss        # oss, team, or strict
      fail_on: error      # error, warn, or never
      post_comment: true  # Post PR comment with findings
    permissions:
      pull-requests: write  # Required for PR comments
```

#### Manual Setup

```yaml
- name: Install builddiag
  uses: taiki-e/install-action@v2
  with:
    tool: builddiag
- name: builddiag (repo truth)
  run: |
    builddiag check \
      --out artifacts/builddiag/report.json \
      --md artifacts/builddiag/comment.md \
      --annotations github
```

### Pre-commit Hook

builddiag can be used as a pre-commit hook to validate your build contract before commits:

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/EffortlessMetrics/builddiag
    rev: main  # or a specific version tag
    hooks:
      - id: builddiag
```

Available hook variants:

| Hook ID | Profile | Description |
|---------|---------|-------------|
| `builddiag` | oss | Default open-source profile (warn-heavy) |
| `builddiag-team` | team | Team profile with stronger gating |
| `builddiag-strict` | strict | Strict profile with maximum enforcement |

The hook triggers on changes to `Cargo.toml`, `rust-toolchain.toml`, or `builddiag.toml`.

Install pre-commit and set up the hooks:

```bash
pip install pre-commit
pre-commit install
```

### IDE Integration

builddiag can output diagnostics in a format compatible with VS Code and other editors:

```bash
builddiag check --format diagnostics
```

This outputs findings in JSON Lines format compatible with VS Code's problem matcher. You can configure a VS Code task to run builddiag and display findings in the Problems panel.

Example `.vscode/tasks.json`:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "builddiag",
      "type": "shell",
      "command": "builddiag",
      "args": ["check", "--format", "diagnostics"],
      "problemMatcher": {
        "owner": "builddiag",
        "fileLocation": ["relative", "${workspaceFolder}"],
        "pattern": {
          "regexp": "^(.*):(\\d+):(\\d+):\\s+(error|warning|info):\\s+\\[(.+)\\]\\s+(.*)$",
          "file": 1,
          "line": 2,
          "column": 3,
          "severity": 4,
          "code": 5,
          "message": 6
        }
      }
    }
  ]
}
```

## Output Formats

builddiag supports three output formats via `--format`:

| Format | Schema | Use Case |
|--------|--------|----------|
| `builddiag` (default) | `builddiag.report.v1` | Native format for direct consumption |
| `sensor` | `sensor.report.v1` | Cockpit CI governance bus envelope |
| `diagnostics` | N/A | IDE-compatible `path:line:col: severity: message` lines |

The **sensor** format wraps the native report in a Cockpit-compatible envelope with structured verdicts, fingerprinted findings, capability tracking, and artifact references.

```bash
builddiag check --format sensor --out report.json
```

## Artifacts Directory Mode

The `--artifacts-dir` flag writes a complete artifact bundle suitable for CI archival:

```bash
builddiag check --artifacts-dir artifacts/builddiag
```

This produces:

```
artifacts/builddiag/
├── report.json          # sensor.report.v1 envelope
├── comment.md           # Markdown summary (PR comment)
└── extras/
    └── payload.json     # builddiag.report.v1 native payload
```

When `--artifacts-dir` is set, `--format`, `--out`, and `--md` are overridden automatically.

## Exit Code Modes

The `--mode` flag controls exit code semantics:

| Mode | Exit 0 | Exit 1 | Exit 2 |
|------|--------|--------|--------|
| `standard` (default) | Pass or Warn (when fail_on=error) | Runtime error | Policy violation |
| `cockpit` | Report written successfully | Catastrophic failure (no report) | N/A |

**Cockpit mode** is designed for CI pipelines where the downstream system reads the report JSON to determine pass/fail. Even on policy violations or tool errors, builddiag writes an error receipt and exits 0 so the pipeline can continue.

```bash
builddiag check --mode cockpit --artifacts-dir artifacts/builddiag
```

## Profiles

Profiles configure check severities as presets. Select with `--profile` or in config:

| Profile | Philosophy | Typical Use |
|---------|-----------|-------------|
| `oss` (default) | Warn-heavy, low friction | Open source projects, wide adoption |
| `team` | Stronger gating | Organizational repos with discipline |
| `strict` | All checks at error severity | Release discipline, CI/CD gates |

<details>
<summary>Profile severity matrix</summary>

| Check | oss | team | strict |
|-------|-----|------|--------|
| `rust.msrv_defined` | warn | warn | error |
| `rust.msrv_consistent` | error | error | error |
| `rust.toolchain_pinning` | info | warn | error |
| `rust.toolchain_msrv_relation` | warn | error | error |
| `rust.edition_deprecations` | info | warn | error |
| `workspace.resolver_v2` | info | warn | error |
| `workspace.edition_consistent` | warn | error | error |
| `workspace.member_ordering` | info | info | error |
| `workspace.publish_ready` | info | warn | error |
| `deps.*` | info | warn | error |
| `deps.security_advisory` | skip | warn | error |
| `tools.*` | skip | warn | error |

</details>

## Config

Optional config file:

```bash
builddiag check --config builddiag.toml
```

If no config is provided, sensible defaults are used.

## Library Usage

Use `builddiag-core` to embed builddiag in your own tools:

```toml
[dependencies]
builddiag-core = "0.2"
```

```rust
use builddiag_core::{Settings, run};

let settings = Settings::default();
let result = run(&settings)?;
println!("Verdict: {:?}", result.report.verdict);
```

See [docs/integration.md](docs/integration.md) for substrate bridge and advanced patterns.

## Generate JSON Schemas

```bash
cargo run -p xtask -- schema
```

## License

MIT OR Apache-2.0
