//! Fuzz target for rust-toolchain.toml parsing.
//!
//! This fuzz target exercises TOML parsing logic used for rust-toolchain.toml files
//! with arbitrary byte sequences to discover crashes, panics, or unexpected behavior.
//!
//! **Validates: Requirements 5.1**

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // Try to parse as TOML and extract toolchain channel
        // This mimics the parsing logic in builddiag-repo
        if let Ok(v) = toml::from_str::<toml::Value>(input) {
            // Try standard format: [toolchain] channel = "..."
            let _ = v
                .get("toolchain")
                .and_then(|t| t.get("channel"))
                .and_then(|c| c.as_str());

            // Try fallback format: channel = "..."
            let _ = v.get("channel").and_then(|c| c.as_str());
        }
    }
});
