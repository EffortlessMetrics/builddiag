# builddiag

[![CI](https://github.com/AmazonQ/builddiag/actions/workflows/ci.yml/badge.svg)](https://github.com/AmazonQ/builddiag/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/AmazonQ/builddiag/branch/main/graph/badge.svg)](https://codecov.io/gh/AmazonQ/builddiag)

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
builddiag check --github-annotations
```

## Config

Optional config file:

```bash
builddiag check --config builddiag.toml
```

If no config is provided, sensible defaults are used.

## Generate JSON Schemas

```bash
cargo run -p xtask -- schema
```

## License

MIT OR Apache-2.0
