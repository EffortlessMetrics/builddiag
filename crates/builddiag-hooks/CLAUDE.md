# builddiag-hooks

Hook snippet generation for `builddiag init-hooks`.

## Purpose

Provides deterministic rendering of:
- pre-commit local hook YAML snippets
- standalone Git `pre-commit` shell hook script
- Husky `pre-commit` script snippet

## Key Types

- `HookProfile` - `oss`, `team`, `strict`
- `InitHooksSpec` - profile + quick-fail toggle
- `HooksBundle` - rendered command + snippets

## Key Functions

- `build_check_command(spec)` - generate `builddiag check ...` command
- `render_hooks(spec)` - generate all snippets in one pass

## Conventions

- Pure string rendering only; no filesystem I/O
- Deterministic output for stable tests/docs
- Keep scripts POSIX shell compatible
