# depguard

Dependency hygiene checks for Cargo workspaces.

## What this crate checks

- Wildcard dependency versions (`*`)
- Path dependencies missing explicit `version`
- Missed opportunities to inherit from `[workspace.dependencies]`

## Key API

- `check_workspace(root, &Config) -> Result<Vec<Finding>>`

## Design constraints

- Deterministic findings with file/line attribution
- Operates directly on Cargo.toml files and workspace layout
- Small focused API for embedding in other tools (including builddiag)

## Example

```rust,no_run
use camino::Utf8Path;
use depguard::{check_workspace, Config};

let findings = check_workspace(Utf8Path::new("."), &Config::default())?;
println!("{} findings", findings.len());
# Ok::<(), anyhow::Error>(())
```
