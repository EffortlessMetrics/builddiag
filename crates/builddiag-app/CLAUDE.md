# builddiag-app

High-level orchestration layer coordinating all builddiag functionality.

## Purpose

Orchestrates the full check workflow:
- Configuration loading from TOML files
- Repository state loading
- Check execution coordination
- Report generation with metadata
- Atomic output file writing
- Git integration for diff-aware mode

## Key Types

- `CheckRun` - Complete execution result (report, markdown, annotations, exit_code)

## Key Functions

### Orchestration
- `run_check(root, config, changed_files)` - Main workflow: load repo → run checks → build report

### Configuration
- `load_config(path)` - Load and parse builddiag.toml (returns defaults if missing)

### Output
- `write_outputs(run, json_path, md_path)` - Atomically write report files
- `write_atomic(path, content)` - Safe write via temp file + rename

### Git Integration
- `compute_changed_files(root, base, head)` - Uses `git diff` for diff-aware mode
- `get_git_info(root)` - Extract commit SHA, branch, dirty status

## Workflow

```
load_config()
     ↓
load_repo_state()  ←── builddiag-repo
     ↓
run_selected_checks()  ←── builddiag-checks
     ↓
summarize() + determine_verdict()  ←── builddiag-domain
     ↓
Build Report with metadata
     ↓
render_markdown() + render_github_annotations()  ←── builddiag-render
     ↓
write_outputs()
```

## Conventions

- Atomic writes prevent corrupted output on failure
- Git integration fails gracefully (returns None if not a repo)
- Report includes complete metadata: schema version, tool version, timestamps, duration, host info
- Summary statistics computed and embedded in report

## Dependencies

- `builddiag-types`, `builddiag-domain`, `builddiag-repo`, `builddiag-checks`, `builddiag-render`
- External: anyhow, serde_json, chrono, camino, toml

## Testing

- Integration tests with fixture repositories
- Unit tests for config loading and git info extraction
