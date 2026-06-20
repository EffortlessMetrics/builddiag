# builddiag-app

Internal orchestration engine for builddiag runs.

This crate wires repository loading, check execution, report assembly, rendering, and artifact writing.

## What this crate provides

- Config loading (`load_config`)
- Diff-aware changed-file discovery (`compute_changed_files`)
- Check orchestration (`run_check`, `run_check_with_sensor`)
- Receipt/error-receipt construction (re-exported via `builddiag-receipt`)
- Atomic output writing helpers (`write_atomic`, `write_outputs`)

## Notes

- This is an internal crate consumed by `builddiag-cli` and `builddiag-core`.
- API stability is scoped to the workspace.
