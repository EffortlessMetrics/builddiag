# builddiag-watch

Deterministic polling watch loop for rerunning builddiag on file changes.

## What this crate provides

- Recursive polling snapshot over contract-relevant files
- Debounce window handling for change bursts
- Optional clear-screen and status-change notifications
- Optional extra watched files

## Key APIs

- `WatchOptions`
- `run_watch_loop(...)`
- `clear_terminal()`

## Design constraints

- Cross-platform behavior without platform-specific watcher backends
- Stable, testable behavior via explicit poll/debounce settings
