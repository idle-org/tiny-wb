//! Reusable configuration and acquisition building blocks for tiny-wb.
//!
//! The core does not depend on terminal state. Parsers and writers accept data
//! or I/O interfaces so callers can test them without a connected instrument.
//!
//! Disable default features for a library without terminal or native serial
//! dependencies. Enable `serial` for USB access or `mock` for deterministic
//! simulation. The application uses `tui` and `serial` by default.
//!
//! # Decode and export a recorded frame
//!
//! ```
//! use tiny_wb::{config::FrequencyAxis, export::write_csv, protocol::{decode_frame, ScanRequest}};
//!
//! let request = ScanRequest { start_hz: 240_000_000, stop_hz: 960_000_000, points: 2, repeat: 1 };
//! let samples = decode_frame(b"{x\x00\x08x\x00\x10}", &request, 128, FrequencyAxis::Inclusive)?;
//! let mut csv = Vec::new();
//! write_csv(&mut csv, &samples, true)?;
//! assert_eq!(String::from_utf8(csv)?, "frequency_MHz,level_dBm\n240,-64\n960,0\n");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]

pub mod acquisition;
pub mod config;
pub mod connection;
pub mod export;
/// Opt-in deterministic simulator and TCP server support.
#[cfg(feature = "mock")]
pub mod mock;
pub mod protocol;
mod storage;
pub mod transport;
