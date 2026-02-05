## builddiag: ⚠️ Warn (0 errors, 1 warnings)

| severity | check | code | location | message |
|---|---|---|---|---|
| warn | rust.msrv_defined | missing_msrv | Cargo.toml | Missing workspace/package rust-version (MSRV) in Cargo.toml |

Reproduce:
`builddiag check --root .`
