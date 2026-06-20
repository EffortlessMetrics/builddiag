//! Shared test support utilities for builddiag test suites.

pub mod fs;
pub mod repo;

#[cfg(feature = "cli")]
pub mod cli;

pub use fs::write_file;
pub use repo::{is_windows_reserved, make_package_toml, make_workspace_toml};

#[cfg(feature = "cli")]
pub use cli::{builddiag_command, run_with_args};
