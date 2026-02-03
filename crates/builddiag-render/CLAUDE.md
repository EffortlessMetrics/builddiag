# builddiag-render

Output rendering layer for human and machine-readable formats.

## Purpose

Transforms reports into output formats:
- Markdown summaries with findings tables
- GitHub Actions annotations
- Budget-aware truncation for large reports

## Key Types

- `RenderOptions` - Configuration (max_findings, show_info)

## Key Functions

### Markdown
- `render_markdown(report)` - Produces Markdown with default options
- `render_markdown_with_options(report, options)` - Customizable rendering

### GitHub Annotations
- `render_github_annotations(report)` - Default annotation output
- `render_github_annotations_with_options(report, options)` - Customizable

## Output Formats

### Markdown
```markdown
# Builddiag Report ✅

| Severity | Check | Code | Location | Message |
|----------|-------|------|----------|---------|
| error | rust.msrv | E001 | Cargo.toml:5 | MSRV not defined |

*Showing 10 of 25 findings*
```

### GitHub Annotations
```
::error file=Cargo.toml,line=5::[rust.msrv:E001] MSRV not defined
::warning file=src/lib.rs,line=10::[deps:W001] Consider workspace inheritance
```

## Conventions

- Filter info-level findings by default (configurable via show_info)
- Sort findings deterministically (severity desc, check_id, path, line)
- Escape special characters for safe rendering
- Respect GitHub's annotation budget (max 50 per job)
- Truncation notes when findings exceed max_findings

## Dependencies

- `builddiag-types` only
- No external dependencies (pure transformation)

## Testing

- Unit tests for each output format
- Snapshot tests for complex reports
- Edge case tests (empty reports, truncation, escaping)
