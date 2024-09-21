//! A (rather) simple crate to interface with a tinySA
//!
//! As of now this crate is not written as a lib/tui
//! and will not offer any scripting capabilities

#![warn(missing_docs, clippy::missing_docs_in_private_items)]
mod scan;
mod structs;

use crate::scan::scan;

use structs::TinyError;

fn main() -> Result<(), TinyError> {
    // let args = Args::parse();
    scan()
}
