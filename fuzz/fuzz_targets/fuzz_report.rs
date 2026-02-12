//! Fuzz target for Report and Finding JSON deserialization.
//!
//! This fuzz target exercises JSON parsing paths for the main report types
//! to discover crashes, panics, or unexpected behavior during deserialization.
//!
//! **Validates: Requirements 5.5 (JSON parsing resilience)**

#![no_main]

use libfuzzer_sys::fuzz_target;

use builddiag_types::{Config, Finding, Report};

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string, handling invalid UTF-8 gracefully
    if let Ok(input) = std::str::from_utf8(data) {
        // Try to parse as Report JSON - should never panic
        let _ = serde_json::from_str::<Report>(input);

        // Try to parse as Finding JSON - should never panic
        let _ = serde_json::from_str::<Finding>(input);

        // Try to parse as array of findings - common in reports
        let _ = serde_json::from_str::<Vec<Finding>>(input);

        // Try to parse as Config JSON (alternative to TOML)
        let _ = serde_json::from_str::<Config>(input);
    }
});
