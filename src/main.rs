//! Startup wiring for the interactive tinySA acquisition application.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]
mod error;
mod scan;
mod ui;

use error::AppError;
use flexi_logger::{FileSpec, Logger, WriteMode};
use std::{ffi::OsString, fs::File, io::BufReader, path::PathBuf};
use tiny_wb::config::{Backend, Config};

/// Startup route selected by the small command-line surface.
#[derive(Debug, PartialEq, Eq)]
enum StartMode {
    /// Configure and acquire a new scan.
    Acquire,
    /// Open an existing CSV directly in the renderer.
    View(PathBuf),
    /// Print command help without loading configuration.
    Help,
}

/// Interpret zero arguments as acquisition and one CSV path as direct viewing.
fn parse_start_mode(args: impl IntoIterator<Item = OsString>) -> Result<StartMode, AppError> {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Ok(StartMode::Acquire),
        [flag] if flag == "-h" || flag == "--help" => Ok(StartMode::Help),
        [flag, path] if flag == "--view" => Ok(StartMode::View(PathBuf::from(path))),
        [path] if !path.to_string_lossy().starts_with('-') => {
            Ok(StartMode::View(PathBuf::from(path)))
        }
        _ => Err(AppError::Usage("usage: tiny-wb [--view] [SCAN.csv]".into())),
    }
}

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
    let mode = parse_start_mode(std::env::args_os().skip(1))?;
    if mode == StartMode::Help {
        println!("tiny-wb [--view] [SCAN.csv]\n\nWith no CSV, acquire a scan. With a CSV path, open the plot renderer directly.");
        return Ok(());
    }
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
    let mut app = match mode {
        StartMode::Acquire => {
            let ports = match config.connection.backend {
                Backend::Serial => serialport::available_ports()?,
                Backend::Tcp => vec![serialport::SerialPortInfo {
                    port_name: format!("tcp://{}", config.connection.tcp_address),
                    port_type: serialport::SerialPortType::Unknown,
                }],
            };
            ui::App::new(ports, config, config_path)
        }
        StartMode::View(path) => {
            let file = File::open(&path).map_err(|source| AppError::Csv {
                path: path.clone(),
                source,
            })?;
            let samples = tiny_wb::export::read_csv(BufReader::new(file)).map_err(|source| {
                AppError::Csv {
                    path: path.clone(),
                    source,
                }
            })?;
            ui::App::viewer(
                config,
                path,
                samples
                    .into_iter()
                    .map(|sample| (sample.frequency_mhz, sample.level_dbm))
                    .collect(),
            )
        }
        StartMode::Help => unreachable!("help returned before configuration loading"),
    };
    let _session = TerminalSession;
    let terminal = ratatui::try_init()?;
    app.run(terminal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_selects_acquisition_help_and_csv_viewer() {
        assert_eq!(parse_start_mode([]).unwrap(), StartMode::Acquire);
        assert_eq!(
            parse_start_mode([OsString::from("--help")]).unwrap(),
            StartMode::Help
        );
        assert_eq!(
            parse_start_mode([OsString::from("scan.csv")]).unwrap(),
            StartMode::View(PathBuf::from("scan.csv"))
        );
        assert_eq!(
            parse_start_mode([OsString::from("--view"), OsString::from("scan.csv")]).unwrap(),
            StartMode::View(PathBuf::from("scan.csv"))
        );
        assert!(parse_start_mode([OsString::from("--unknown")]).is_err());
    }
}
