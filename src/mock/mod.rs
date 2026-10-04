//! Deterministic tinySA console simulation for tests and local integrations.
//!
//! Enable the `mock` feature. This is a command/state simulator, not an RF model
//! or an emulator of a specific firmware release. [`crate::mock::MockStream`] and the TCP
//! server share the same parser and device state; neither needs serial drivers.

//! # Inspect state after a command
//!
//! ```
//! use tiny_wb::{mock::{MockConfig, MockStream}, transport::Shell};
//! use std::time::Duration;
//! let mut device = MockStream::new(MockConfig::default())?;
//! {
//!     let mut shell = Shell::new(&mut device, 65_536);
//!     shell.synchronize()?;
//!     shell.command_checked("pause", Duration::ZERO, Duration::ZERO)?;
//! }
//! assert!(device.state().paused);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod device;
mod server;

pub use device::{MockConfig, MockFault, MockModel, MockState, MockStream};
pub use server::serve_connection;
