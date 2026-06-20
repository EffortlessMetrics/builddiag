# builddiag-checks-catalog

This microcrate owns the check metadata and built-in check registry for builddiag.
It intentionally has a narrow public API and no filesystem/runtime dependencies so it
can be shared across check implementations, CLI presentation, and BDD tooling.

## Features

- `msrv`: include MSRV checks (`rust.msrv_defined`, `rust.msrv_consistent`)
- `toolchain`: include toolchain checks (`rust.toolchain_*`)
- `checksums`: include checksum checks (`tools.checksums_*`)
- `workspace`: include workspace checks (`workspace.*`, `rust.edition_deprecations`)
- `deps`: include dependency checks (`deps.*`, `workspace.publish_ready` not included)
- `publish`: include publish/readiness check (`workspace.publish_ready`)

All features are enabled by default.
