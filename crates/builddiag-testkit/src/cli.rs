//! CLI command helpers for integration and BDD tests.

use assert_cmd::Command;
use std::path::Path;
use std::process::Output;

/// Construct the `builddiag` cargo binary command.
#[allow(deprecated)] // cargo_bin works correctly for library test helpers; the macro alternative requires CARGO_BIN_EXE_ env vars only available in integration test crates
pub fn builddiag_command() -> Command {
    Command::cargo_bin("builddiag").expect("failed to resolve builddiag cargo binary")
}

/// Run `builddiag` from a working directory with string-like args and return the output.
pub fn run_with_args<I, S>(dir: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cmd = builddiag_command();
    cmd.current_dir(dir);
    for arg in args {
        cmd.arg(arg.as_ref());
    }
    cmd.output().expect("failed to execute builddiag")
}
