# builddiag Integration

This guide shows common integration patterns for builddiag in CI and local developer workflows.

## GitHub Actions

Use the prebuilt binary and write canonical artifacts.

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

## Local Dev Hooks

A simple pre-commit hook that runs builddiag locally:

```bash
cat > .git/hooks/pre-commit <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
builddiag check \
  --out artifacts/builddiag/report.json \
  --md artifacts/builddiag/comment.md
EOF
chmod +x .git/hooks/pre-commit
```

## Pre-commit Framework

If you use the `pre-commit` framework, add a local hook:

```yaml
repos:
  - repo: local
    hooks:
      - id: builddiag
        name: builddiag
        entry: builddiag check --out artifacts/builddiag/report.json --md artifacts/builddiag/comment.md
        language: system
        pass_filenames: false
```
