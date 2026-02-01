//! Fuzz target for Config TOML parsing.
//!
//! This fuzz target exercises the Config deserialization from TOML with arbitrary
//! byte sequences to discover crashes, panics, or unexpected behavior.
//!
//! **Validates: Requirements 5.4**

#![no_main]

use builddiag_types::Config;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // Try to parse as Config TOML - should never panic
        let _ = toml::from_str::<Config>(input);
    }
});
