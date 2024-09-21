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
}
