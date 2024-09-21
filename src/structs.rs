//! All the structs used internally
use thiserror::Error;

/// The most common error, exposed to the lib user and end used
#[derive(Error, Debug)]
pub enum TinyError {
    /// An error occured while receiving the raw data and '{' couldn't be found
    #[error("Couldn't parse the beggining of the data ({0})")]
    ParseBeggining(String),
    /// An error occured while receiving the raw data and '}' couldn't be found
    #[error("Couldn't parse the end of the data ({0})")]
    ParseEnd(String),
    /// The given serial port couldn't be opened
    #[error("Couldn't connect to the given port")]
    PortConnection(#[from] serialport::Error),
    /// The given file could't be exported
    #[error("Couldn't open the given file")]
    OpenOutput(#[from] std::io::Error),
    /// Generic TUI initialization error
    #[error("An error occured while creating the TUI")]
    TUIInitialization(#[from] color_eyre::Report),
    /// A runtime error occured
    #[error("A runtime error occured :\n{0}")]
    TUIRuntimeError(color_eyre::eyre::Error),
}
