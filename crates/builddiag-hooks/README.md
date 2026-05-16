# builddiag-hooks

Deterministic hook snippet generation used by `builddiag init-hooks`.

## What this crate provides

- Pre-commit YAML snippet rendering
- Git `pre-commit` shell hook script rendering
- Husky `pre-commit` script rendering
- Shared command generation (`build_check_command`)

## Key APIs

- `HookProfile`
- `InitHooksSpec`
- `render_hooks(...)`

## Design constraints

- Pure string rendering (no file writes)
- POSIX-shell-compatible scripts
- Stable output for tests and docs
