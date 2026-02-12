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

use builddiag_repo::parse_checksums_content;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // Use the actual production parser - should never panic
        let _ = parse_checksums_content(input);
    }
});
