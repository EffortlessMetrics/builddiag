# builddiag-render

Renderers for builddiag reports.

## Output formats

- Markdown summary for PR comments
- GitHub Actions annotation lines
- IDE diagnostics lines (`path:line:col: severity: message`)

## Key APIs

- `render_markdown(...)`
- `render_github_annotations(...)`
- `render_diagnostics(...)`
- `RenderOptions` (`max_findings`, `show_info`)

## Design constraints

- Deterministic finding ordering
- Budget-aware truncation for large reports
- No filesystem/process I/O
