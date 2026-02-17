# builddiag

Command-line tool to validate the build contract of Rust repositories.

## Install

```bash
cargo install builddiag
```

## Run

```bash
builddiag check \
  --out artifacts/builddiag/report.json \
  --md artifacts/builddiag/comment.md \
  --annotations github

# Baseline workflow
builddiag baseline create --root .
builddiag check --baseline .builddiag-baseline.json
builddiag baseline update --root .
```
