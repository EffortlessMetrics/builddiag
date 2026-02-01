//! Fuzz target for checksums file parsing.
//!
//! This fuzz target exercises the checksums file parsing logic with arbitrary
//! byte sequences to discover crashes, panics, or unexpected behavior.
//!
//! The checksums format is: `<hash><whitespace><path>` per line, with comments
//! starting with `#` and empty lines ignored.
//!
//! **Validates: Requirements 5.2**

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Parse checksums content in the same way as builddiag-repo::parse_checksums
fn parse_checksums_content(content: &str) -> Vec<(usize, String, String)> {
    let mut entries = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse format: <hash><whitespace><path>
        let mut parts = trimmed.split_whitespace();
        let hash = match parts.next() {
            Some(h) => h.to_string(),
            None => continue,
        };
        let path = parts.next().map(|p| p.to_string()).unwrap_or_default();

        entries.push((line_no, hash, path));
    }

    entries
}

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // The parsing function should never panic
        let _ = parse_checksums_content(input);
    }
});
