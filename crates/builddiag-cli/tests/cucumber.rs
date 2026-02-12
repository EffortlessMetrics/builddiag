//! Cucumber BDD test runner for builddiag-cli.
//!
//! This test binary runs all Gherkin feature files in the `tests/features/` directory.
//!
//! Run with:
//! ```bash
//! cargo test --test cucumber
//! cargo test --test cucumber -- --tags @msrv
//! ```

mod bdd;

use bdd::world::BuilddiagWorld;
use cucumber::World;

#[tokio::main]
async fn main() {
    BuilddiagWorld::run("tests/features").await;
}
