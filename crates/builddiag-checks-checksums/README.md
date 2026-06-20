# builddiag-checks-checksums

Checksum-focused checks for `builddiag`.

## Checks

- `tools.checksums_file_exists`
- `tools.checksums_format`
- `tools.checksums_coverage`
- `tools.checksums_verify_local`

The crate keeps checksum policy and verification logic separated from the main
`builddiag-checks` orchestration layer to keep responsibilities focused.

