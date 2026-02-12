# depguard

Dependency hygiene checker for Cargo.toml files.

## Purpose

Validates Cargo.toml dependency declarations:
- Detects wildcard version specifications
- Ensures path dependencies have versions for publishing
- Suggests workspace inheritance opportunities

## Checks

### wildcard_version
Detects `foo = "*"` specifications which allow any version.
```toml
# Bad
serde = "*"

# Good
serde = "1.0"
```

### path_missing_version
Path dependencies without version can't be published to crates.io.
```toml
# Bad
my-crate = { path = "../my-crate" }

# Good
my-crate = { path = "../my-crate", version = "0.1" }
```

### missing_workspace_inheritance
Suggests using `foo.workspace = true` when the dependency exists in `[workspace.dependencies]`.
```toml
# In workspace Cargo.toml
[workspace.dependencies]
serde = "1.0"

# In member Cargo.toml - suggests inheritance
serde = "1.0"  # Could be: serde.workspace = true
```

## Key Types

- `Finding` - Result (severity, code, message, path, line)
- `Config` - Which checks to run and dependencies to ignore

## Key Functions

- `check_workspace(root, config)` - Scan all crates for dependency issues

## Usage from builddiag-checks

```rust
use depguard::{check_workspace, Config};

let config = Config {
    check_wildcard: true,
    check_path_version: true,
    check_workspace_inheritance: true,
    ignore: vec!["internal-crate".to_string()],
};

let findings = check_workspace(&root, &config)?;
```

## Conventions

- Walks workspace respecting glob patterns in `[workspace.members]`
- Parses `[workspace.dependencies]` for inheritance suggestions
- Returns line numbers for precise finding locations
- Respects ignore list for internal/special dependencies

## Dependencies

- External: anyhow, serde, serde_json, camino, toml, semver, globset

## Testing

- Unit tests per check type
- Fixture workspaces for integration testing
