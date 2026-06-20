//! Output contract validation crate for `builddiag` reports.
//!
//! This crate provides deterministic validation entry points for serializable report
//! contracts produced by the workspace.

#[cfg(feature = "report")]
pub mod report;

#[cfg(feature = "report")]
pub use report::{
    load_and_validate_builddiag_report, load_builddiag_report, validate_builddiag_report,
};

#[cfg(feature = "sensor")]
pub mod sensor;

#[cfg(feature = "sensor")]
pub use sensor::{load_and_validate_sensor_report, load_sensor_report, validate_sensor_report};
