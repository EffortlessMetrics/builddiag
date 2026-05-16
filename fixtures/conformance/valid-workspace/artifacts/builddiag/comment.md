## builddiag: ⚠️ Warn (0 errors, 1 warnings)

| severity | check | code | location | message |
|---|---|---|---|---|
| warn | rust.toolchain_msrv_relation | toolchain_msrv_mismatch | rust-toolchain.toml | Toolchain (1.85.0) must equal MSRV (1.92.0) |

Reproduce:
`builddiag check --root .`
