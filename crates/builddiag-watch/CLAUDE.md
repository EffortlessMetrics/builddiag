# builddiag-watch

Polling watch-loop utilities for `builddiag watch`.

## Purpose

Provide a deterministic, cross-platform file watch loop without requiring
platform-specific watcher backends.

## Behavior

- Polls for changes in:
  - `Cargo.toml`
  - `rust-toolchain.toml`
  - `checksums.txt`
- Supports debounce and poll interval tuning.
- Supports optional terminal clear and status-change bell notifications.
- Supports extra watched files (for example explicit config files).

## API

- `WatchOptions` - Runtime watch configuration.
- `run_watch_loop(options, run_once)` - Executes initial run, then re-runs on change.
- `clear_terminal()` - ANSI clear helper.

## Notes

- The loop is infinite unless `max_runs` is set.
- No CLI parsing lives here; CLI mapping belongs in `builddiag-cli`.
