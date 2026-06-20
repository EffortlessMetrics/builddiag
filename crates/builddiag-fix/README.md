# builddiag-fix

Deterministic auto-fix planning and apply logic for `builddiag fix`.

## What this crate provides

- Non-destructive planning (`plan_fixes`)
- Apply workflow with `dry_run` and optional interactive confirmation (`apply_fixes`)
- Stable proposal ordering and warnings for ambiguous cases

## Current fix kinds

- Add `workspace.package.rust-version` when derivable
- Set `workspace.resolver = "2"`
- Add missing checksum entries for declared tools

## Design constraints

- No guessing for ambiguous input
- Deterministic outputs for reproducible dry-run UX
- CLI prompting/argument parsing stays outside this crate
