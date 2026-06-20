# builddiag-core

Public, Clap-free library API for running builddiag in-process.

## What this crate provides

- Main entry point: `run(&Settings)`
- Config loading helper: `load_config(...)`
- Access to both native and sensor reports in one run result
- Optional substrate input path for callers with precomputed manifests

## Core types

- `Settings`
- `RunResult`

## Features

- `cache` (default): enables cached repo-state loading via `builddiag-repo`

## Example

```rust,ignore
use builddiag_core::{run, Settings};

let result = run(&Settings::default())?;
println!("Verdict: {:?}", result.report.verdict);
```
