//! Startup wiring for the interactive tinySA acquisition application.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]
mod error;
mod scan;
mod ui;

use error::AppError;
use flexi_logger::{FileSpec, Logger, WriteMode};
use std::path::PathBuf;
use tiny_wb::config::{Backend, Config};

/// Restore the terminal on ordinary errors and unwinding after initialization.
struct TerminalSession;

impl Drop for TerminalSession {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Load and validate settings before taking ownership of the user's terminal.
fn main() -> Result<(), AppError> {
    color_eyre::install()?;
    let config_path = std::env::var_os("TINY_WB_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));
    let config = Config::load(&config_path).map_err(|error| {
        eprintln!("Cannot load {}: {error}", config_path.display());
        error
    })?;
    let _logger = Logger::try_with_str(&config.logging.filter)?
        .log_to_file(FileSpec::default().directory(&config.logging.directory))
        .write_mode(WriteMode::BufferAndFlush)
        .start()?;
    let ports = match config.connection.backend {
        Backend::Serial => serialport::available_ports()?,
        Backend::Tcp => vec![serialport::SerialPortInfo {
            port_name: format!("tcp://{}", config.connection.tcp_address),
            port_type: serialport::SerialPortType::Unknown,
        }],
    };
    let _session = TerminalSession;
    let terminal = ratatui::try_init()?;
    ui::App::new(ports, config, config_path).run(terminal)?;
    Ok(())
}
