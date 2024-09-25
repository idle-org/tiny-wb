//! A (rather) simple crate to interface with a tinySA
//!
//! As of now this crate is not written as a lib/tui
//! and will not offer any scripting capabilities

#![warn(missing_docs, clippy::missing_docs_in_private_items)]
// #![allow(dead_code)]
mod scan;
mod structs;
mod ui;
mod ui_struct;

use color_eyre::Result;
// use human_panic::setup_panic;
use structs::TinyError;
use ui_struct::AppInfo;

use flexi_logger::{FileSpec, Logger, WriteMode};
use log::info;

fn main() -> Result<(), TinyError> {
    let _logger = Logger::try_with_str("info, my::critical::module=trace")
        .unwrap()
        .log_to_file(FileSpec::default())
        .write_mode(WriteMode::BufferAndFlush)
        .start()
        .unwrap();

    info!(target:"info","Logging started");
    color_eyre::install()?;
    let terminal = ratatui::init();
    // setup_panic!();
    let ports = serialport::available_ports()?;
    let app_r = AppInfo::new(ports).run(terminal);

    ratatui::restore();

    app_r.map_err(TinyError::TUIRuntimeError)
}
