//! Reusable configuration and acquisition building blocks for tiny-wb.
//!
//! The core does not depend on terminal state. Parsers and writers accept data
//! or I/O interfaces so callers can test them without a connected instrument.

#![warn(missing_docs)]

pub mod acquisition;
pub mod config;
pub mod export;
pub mod protocol;
mod storage;
pub mod transport;
