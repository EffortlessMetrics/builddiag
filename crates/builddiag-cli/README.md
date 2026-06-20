# builddiag (CLI crate)

Command-line entry point for builddiag.

This crate builds the `builddiag` binary and wires command parsing to the workspace libraries.

## Install

From this workspace:

```bash
cargo install --path crates/builddiag-cli
```

From crates.io:

```bash
cargo install builddiag
```

## Commands

- `builddiag check` - run checks and emit reports
- `builddiag watch` - rerun checks on file changes
- `builddiag fix` - plan/apply deterministic auto-fixes
- `builddiag md` - render Markdown from report JSON
- `builddiag annotations` - render GitHub annotations from report JSON
- `builddiag explain` - explain check ids and finding codes
- `builddiag list-checks` - list registered checks
- `builddiag init-hooks` - print/install hook snippets
- `builddiag baseline create|update` - baseline snapshot workflows

## Typical usage

```bash
builddiag check --root . --out artifacts/builddiag/report.json --md artifacts/builddiag/comment.md
```

Inline suppression comments are supported in `Cargo.toml` via `# builddiag:ignore <selector>`.
