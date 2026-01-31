# builddiag

A Rust CLI tool that validates the "build contract" of Rust repositories. It performs static analysis of manifests and policy files to check:

- MSRV (Minimum Supported Rust Version) configuration
- Toolchain pinning and version consistency
- Tool checksums verification
- Workspace configuration

Designed to be fast and offline - reads manifests only, no cargo commands executed.

## Output Formats

- JSON report (`report.json`) - machine-readable
- Markdown summary (`comment.md`) - PR comment friendly
- GitHub Actions annotations - CI integration

## Use Cases

- CI/CD pipelines to enforce build policies
- Pre-commit validation of Rust project configuration
- Automated PR checks for configuration drift
