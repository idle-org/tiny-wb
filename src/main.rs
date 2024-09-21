//! A (rather) simple crate to interface with a tinySA
//!
//! As of now this crate is not written as a lib/tui
//! and will not offer any scripting capabilities

#![warn(missing_docs, clippy::missing_docs_in_private_items)]
#![allow(dead_code)]
mod scan;
mod structs;
mod ui;

use color_eyre::Result;
use structs::TinyError;
use ui::AppInfo;

fn main() -> Result<(), TinyError> {
    // let args = Args::parse();
    // scan()

    color_eyre::install()?;
    let terminal = ratatui::init();
    let ports = serialport::available_ports()?;
    let app_r = AppInfo::new(ports).run(terminal);

    ratatui::restore();

    app_r.map_err(TinyError::TUIRuntimeError)
}
