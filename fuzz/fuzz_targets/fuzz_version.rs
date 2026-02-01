//! Fuzz target for version string parsing.
//!
//! This fuzz target exercises the `parse_rust_version` function from builddiag-domain
//! with arbitrary byte sequences to discover crashes, panics, or unexpected behavior.
//!
//! **Validates: Requirements 5.3**

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // The function should never panic, only return Ok or Err
        let _ = builddiag_domain::parse_rust_version(input);
    }
});
