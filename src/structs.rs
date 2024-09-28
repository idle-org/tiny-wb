//! All the structs used internally

use std::{
    fs::File,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

use flexi_logger::FlexiLoggerError;
use log;
use serde;
use thiserror;

use crate::ui_struct::AppInfo;

/// The most common error, exposed to the lib user and end used
#[derive(thiserror::Error, Debug)]
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
    #[error("The given scan frequencies are inccorect ({0})")]
    BadScanFrequencies(String),
    /// Couldn't initialize the logging modure
    #[error("Couldn't initialize the logging modure")]
    LoggerError(#[from] FlexiLoggerError),
    /// Couldn't send data
    #[error("Couldn't write to the TinySA, connection lost")]
    SendError(std::io::Error),
}

#[allow(missing_docs, clippy::missing_docs_in_private_items)]
/// The direction the input was received, usefull for menu where R/L U/D give similar results and implementation
pub(crate) enum InputDirection {
    Up,
    Down,
    Right,
    Left,
}

// TEST: Easy
/// All implementation of errors happening while in the UI
#[derive(thiserror::Error, Debug)]
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
    /// Couldn't get a port from the selection
    #[error("No port was selected")]
    NoPortSelected,
    /// Any Scan Error
    #[error("An error occured while trying to scan: \n => {0}")]
    ScanError(#[from] TinyError),
    /// No connection object was created
    #[error("An error occured while trying to acquire the connection info structure")]
    NoConnectionInfoCreated,
    /// No return data was created or initialized
    #[error("An error occured while trying to acquire the return data")]
    NoReturnData,
    /// No error was generated, this error is a meta error
    #[error("An unspecified error occured")]
    NoErrorSpecified,
    /// Error occured while tring to serialize the config to file
    #[error("Couldn't serialize the error to file")]
    SerializeError(#[from] toml::ser::Error),
}

// TEST: Easy
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct SavedConfig {
    /// The default com port
    pub(crate) port: String,
    /// The default baudrate
    pub(crate) baudrate: u32,
    /// The default start frequency
    pub(crate) start_frequency: usize,
    /// The default stop frequency
    pub(crate) stop_frequency: usize,
    /// The default number of points
    pub(crate) points: usize,
    /// The default number of repeat per point
    pub(crate) repeat: usize,
}
// TEST: Easy
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
        f.write_all(
            toml::to_string(&Self {
                port: arg.port_list.selected_port()?,
                baudrate: arg.baudrate.selected_baudrate()?,
                start_frequency: arg.start_frequency.int_to_usize(),
                stop_frequency: arg.stop_frequency.int_to_usize(),
                points: arg.points.int_to_usize(),
                repeat: arg.repeat.int_to_usize(),
            })?
            .as_bytes(),
        )?;
        Ok(())
    }
}

// TEST: Easy
/// The various status the connection can take
#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectionStatus {
    /// The connection was created with no initialisation data
    Uninitialised,
    /// The connection has initialisation data but wasn't established
    Initialised,
    /// The connection was established
    Connected,
    /// The scan was started
    ScanStarted,
    /// The connection coudn't be done, with a string explaining the error
    ConnectionFailure,
    /// The connection is closed and the connection was a succes
    ClosedWithSuccess,
}

type ScanData = Vec<(f64, f64)>;

// TEST: Easy
#[derive(Debug, Clone)]
/// All data needed to handle a connection to the TinySA
pub(crate) struct ConnectionInfo {
    /// The number of points to scan
    pub(crate) points: usize,
    /// The baudrate of the connection
    pub(crate) baudrate: u32,
    /// The Serial port to open for connection
    pub(crate) port: String,
    /// A mutex to share the progression (over 100)
    progress: Arc<Mutex<usize>>,
    /// The number of times to repeate a mesurement of a point
    pub(crate) repeat: usize,
    /// A mutex to the result of the scan
    return_data: Arc<Mutex<Option<ScanData>>>,
    /// The information of the TinySA
    scan_info: Arc<Mutex<String>>,
    /// The last scan message (what's happening as the scan goes)
    scan_message: Arc<Mutex<String>>,
    /// The value to start at (in Hz)
    pub(crate) start: usize,
    /// The value to stop at (in Hz)
    pub(crate) stop: usize,
    /// The file name to store all data
    file_name: Arc<Mutex<String>>,
    /// The connection status
    connection_status: Arc<Mutex<ConnectionStatus>>,
    /// The last error raised
    pub(crate) error_raised: Arc<Mutex<Option<TinyUIError>>>,
}

// TEST: Easy
impl ConnectionInfo {
    /// Create the initial connection containing only the initial mutex for error collection
    pub(crate) fn new() -> Self {
        Self::default()
    }
    /// Updates the connection with the given data
    pub(crate) fn fill(
        &mut self,
        port: String,
        baudrate: u32,
        start: usize,
        stop: usize,
        points: usize,
        repeat: usize,
    ) {
        self.port = port;
        self.baudrate = baudrate;
        self.start = start;
        self.stop = stop;
        self.points = points;
        self.repeat = repeat;
    }

    /// Updates the generic scan message
    pub(crate) fn update_message(&self, message: &'static str) {
        log::trace!("Message: {message}");
        *self.scan_message.lock().unwrap() = message.to_string();
    }

    /// Updates the scan progress
    pub(crate) fn update_progress(&self, progress: usize) {
        log::trace!("Progress: {progress}");
        *self.progress.lock().unwrap() = progress;
    }

    /// Update the information of the TinySA
    pub(crate) fn update_info(&self, message: String) {
        log::info!("Info: {message}");
        *self.scan_info.lock().unwrap() = message.to_string();
    }

    /// A clone of the mutex to keep it updated
    pub(crate) fn mutex_progress(&self) -> Arc<Mutex<usize>> {
        self.progress.clone()
    }

    /// Update the return data with the new points
    pub(crate) fn update_return_data(&self, return_data: Vec<(f64, f64)>) {
        *self.return_data.lock().unwrap() = Some(return_data);
    }

    /// Update the filename with the one choosen by the save method
    pub(crate) fn update_filename(&self, name: String) {
        *self.file_name.lock().unwrap() = name;
    }

    /// Get the current progression level
    pub(crate) fn get_progress(&self) -> usize {
        *self.progress.lock().unwrap()
    }

    /// Get the latest scan message
    pub(crate) fn get_scan_message(&self) -> String {
        (*self.scan_message.lock().unwrap()).to_string()
    }

    /// Get the latest tinySA information (There is no guaranty it is initialised or valid)
    pub(crate) fn get_scan_info(&self) -> String {
        (*self.scan_info.lock().unwrap()).to_string()
    }

    /// Get the latest connection status
    pub(crate) fn get_connection_status(&self) -> ConnectionStatus {
        *self.connection_status.lock().unwrap()
    }

    /// Take the return data and leave none in place
    pub(crate) fn get_return_data(&self) -> Result<Vec<(f64, f64)>, TinyUIError> {
        self.return_data
            .lock()
            .unwrap()
            .take()
            .ok_or(TinyUIError::NoReturnData)
    }

    /// Set an error
    pub(crate) fn set_error(&self, e: Option<TinyUIError>) {
        log::error!("Error: {e:?}");
        *self.error_raised.lock().unwrap() = e;
    }

    /// Wether an error has been raised
    pub(crate) fn has_error(&self) -> bool {
        self.error_raised.lock().unwrap().is_some()
    }

    /// Manually sets the connections status
    pub(crate) fn set_connection_status(&self, status: ConnectionStatus) {
        log::info!("Status: {status:?}");
        *self.connection_status.lock().unwrap() = status;
    }

    // pub(crate) fn get_error(&self) -> &Option<TinyError> {
    //     self.error_raised.lock().as_ref().unwrap()
    // }
}

// TEST: Easy
impl Default for ConnectionInfo {
    fn default() -> Self {
        Self {
            points: 11,
            baudrate: 115_200,
            port: String::new(),
            progress: Arc::new(Mutex::new(0)),
            repeat: 1,
            return_data: Arc::new(Mutex::new(None)),
            scan_info: Arc::new(Mutex::new(String::new())),
            scan_message: Arc::new(Mutex::new(String::new())),
            start: 240,
            stop: 960,
            file_name: Arc::new(Mutex::new(String::new())),
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Uninitialised)),
            error_raised: Arc::new(Mutex::new(None)),
        }
    }
}
