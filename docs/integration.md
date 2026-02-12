# builddiag Integration

This guide shows common integration patterns for builddiag in CI, local developer workflows, and as a library dependency.

## Library Usage (builddiag-core)

Use `builddiag-core` to embed builddiag in your own tools without any CLI dependency:

```toml
[dependencies]
builddiag-core = { version = "0.2", features = [] }
```

```rust
use builddiag_core::{Settings, run};

let settings = Settings::default();
let result = run(&settings)?;
println!("Verdict: {:?}", result.report.verdict);
println!("Sensor schema: {}", result.sensor_report.schema);
```

### Substrate Bridge (Zero I/O)

If your tool has already parsed the workspace, supply a `Substrate` to skip disk reads:

```rust
use builddiag_core::{Settings, run, SubstrateType, ManifestInfo};

let substrate = SubstrateType {
    manifests: vec![ManifestInfo {
        path: "Cargo.toml".to_string(),
        name: Some("my-crate".to_string()),
        msrv: Some("1.75".to_string()),
        edition: Some("2021".to_string()),
    }],
    has_toolchain: true,
    toolchain_channel: Some("1.75.0".to_string()),
    has_checksums: false,
    has_lockfile: true,
    workspace_msrv: Some("1.75".to_string()),
};

let settings = Settings {
    substrate: Some(substrate),
    ..Default::default()
};
let result = run(&settings)?;
```

### Dual Report Output

`run()` returns both report formats:
- `result.report` — native `builddiag.report.v1`
- `result.sensor_report` — Cockpit CI `sensor.report.v1` envelope

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

### Artifacts Directory Mode

Use `--artifacts-dir` for Cockpit-compatible artifact layout:

```yaml
- name: builddiag (sensor mode)
  run: |
    builddiag check --artifacts-dir artifacts/builddiag
    # Produces:
    #   artifacts/builddiag/report.json         (sensor.report.v1)
    #   artifacts/builddiag/extras/payload.json  (builddiag.report.v1)
    #   artifacts/builddiag/comment.md
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
