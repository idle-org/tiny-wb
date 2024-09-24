//! All the structs used internally

use std::{
    fs::File,
    io::{Read, Write},
};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::ui_struct::{AppInfo, BaudRate};

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
    /// An error occured while trying to parse the result
    #[error("An error occured while trying to parse the result")]
    TUIPackingError(#[from] packed_struct::PackingError),
    /// The config file couldn't be opened
    #[error("The config file couldn't be opened")]
    ConfigNotFound,
    /// The config file couldn't be parsed as a valid toml
    #[error("The config config.toml couldn't be read")]
    InvalidConfig(#[from] toml::de::Error),
    /// The config file couldn't be parsed as a valid toml
    #[error("The config config.toml couldn't be written")]
    InvalidDataForConfig(#[from] toml::ser::Error),
    /// The version couldn't be acquired, likely a bogus connection
    #[error("Couldn't acquire the version of the device connected to the specified port")]
    NoVersionFound,
    /// The given scan frequencies are incorrect
    // TODO: Expand
    #[error("The given scan frequencies are inccorect")]
    BadScanFrequencies,
}

#[allow(missing_docs, clippy::missing_docs_in_private_items)]
/// The direction the input was received, usefull for menu where R/L U/D give similar results and implementation
pub(crate) enum InputDirection {
    Up,
    Down,
    Right,
    Left,
}

/// All implementation of errors happening while in the UI
#[derive(Error, Debug)]
pub enum TinyUIError {
    /// An error due to bad menuing implementation
    #[error("An error occured while selecting a menu item")]
    MenuSelectionError,

    /// An error due to bad initialization
    #[error("The item list ({0}) wasn't initialyzed ({2}:{1})")]
    UninitializedItemList(&'static str, u32, &'static str),

    /// An error occured while proccesing the key events
    #[error("An error occured while proccesing the key events")]
    KeyPress(#[from] std::io::Error),
    // /// Mutex error likely a data race mishandled
    // #[allow(dead_code)]
    // #[error("An error occured while trying to fetch progress")]
    // Mutex,
}

// impl TinyUIError {
//     pub(crate) fn uninitialized_item_list_error(list: &'static str) -> Self {
//         Self::UninitializedItemList(list, line!(), file!())
//     }
// }

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct SavedConfig {
    /// The default com port
    pub(crate) port: String,
    /// The default baudrate
    pub(crate) baudrate: usize,
    /// The default start frequency
    pub(crate) start_frequency: usize,
    /// The default stop frequency
    pub(crate) stop_frequency: usize,
    /// The default number of points
    pub(crate) points: usize,
}
impl SavedConfig {
    /// Create a config from file or returns an error
    pub(crate) fn new() -> Result<Self, TinyError> {
        if let Ok(mut f) = File::open("config.toml") {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            let c = toml::from_str(&s)?;
            Ok(c)
        } else {
            Err(TinyError::ConfigNotFound)
        }
    }

    /// Save the config with all runtime parameters
    pub(crate) fn save(arg: &AppInfo) -> Result<(), TinyUIError> {
        let mut f = File::create("config.toml")?;
        // TODO: Prone to failure
        f.write_all(
            toml::to_string(&Self {
                port: arg.port_list.selected_port(),
                baudrate: BaudRate::iter()
                    .nth(arg.baudrate.state.selected().unwrap())
                    .unwrap() as usize,
                start_frequency: arg.start_frequency.int_to_usize(),
                stop_frequency: arg.stop_frequency.int_to_usize(),
                points: arg.points.int_to_usize(),
            })
            .unwrap()
            .as_bytes(),
        )?;
        Ok(())
    }
}

/// The various status the connection can take
#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectionStatus {
    /// The connection was created with no initialisation data
    Uninitialised,
    // /// The connection has initialisation data but wasn't established
    // Initialised,
    // /// The connection was established
    // Connected,
    /// The connection coudn't be done, with a string explaining the error
    ConnectionFailure,
    /// The connection is closed and the connection was a succes
    ClosedWithSuccess,
    // /// The connection is closed with a failure
    // ClosedWithError,
}
