# builddiag-cli

Command-line interface and entry point for builddiag.

## Purpose

Provides the user-facing CLI:
- Argument parsing via clap
- Command routing and execution
- Output formatting and display
- Exit code handling

## Commands

### check
Run validation on a repository.
```bash
builddiag check --root . --config builddiag.toml --profile strict
builddiag check --diff-aware --base main --head HEAD
builddiag check --out report.json --md report.md --annotations
```

Options:
- `--root` - Repository root (default: current directory)
- `--config` - Config file path
- `--profile` - Severity profile: oss, team, strict
- `--out` - JSON report output path
- `--md` - Markdown report output path
- `--annotations` - Emit GitHub Actions annotations to stdout
- `--diff-aware` - Only run checks for changed files
- `--base`, `--head` - Git refs for diff-aware mode
- `--always` - Checks to always run regardless of diff

### watch
Continuously rerun validation when Cargo/toolchain/checksum inputs change.
```bash
builddiag watch --root . --profile strict
builddiag watch --poll-ms 200 --debounce-ms 400 --notify
```

### fix
Apply deterministic fixes for unambiguous issues.
```bash
builddiag fix --root .
builddiag fix --dry-run
builddiag fix --interactive
```

### md
Render Markdown from existing JSON report.
```bash
builddiag md report.json > report.md
```

### annotations
Emit GitHub Actions annotations from JSON report.
```bash
builddiag annotations report.json
```

### explain
Show documentation for a check or finding code.
```bash
builddiag explain rust.msrv_defined
builddiag explain E001
```

### list-checks
Display all available checks with profile severities.
```bash
builddiag list-checks
builddiag list-checks --json
```

### init-hooks
Generate and optionally install pre-commit/Git/Husky hook snippets.
```bash
builddiag init-hooks --profile team
builddiag init-hooks --quick-fail --install
```

## Exit Codes

- `0` - Success (verdict is Pass or Warn with fail_on: error)
- `1` - Runtime error (config parse failure, I/O error)
- `2` - Policy violation (verdict triggers configured fail_on)

## Conventions

- Use clap's derive macros for argument parsing
- Profile CLI arg converts to Config Profile enum
- Respect config file defaults for output paths
- JSON output to file, annotations to stdout

## Dependencies

- `builddiag-app`, `builddiag-watch`, `builddiag-fix`, `builddiag-checks`, `builddiag-domain`, `builddiag-render`, `builddiag-types`
- External: anyhow, clap, camino, serde_json

## Testing

Integration tests in `tests/` using `assert_cmd` and `predicates`:
- Command execution tests
- Exit code verification
- Output format validation
