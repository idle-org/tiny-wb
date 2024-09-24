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

    /// An error occured while trying to parse the result
    #[error("An error occured while trying to parse the result")]
    TUIPackingError(#[from] packed_struct::PackingError),
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

    /// Mutex error likely a data race mishandled
    #[error("An error occured while trying to fetch progress")]
    Mutex,
}

// impl TinyUIError {
//     pub(crate) fn uninitialized_item_list_error(list: &'static str) -> Self {
//         Self::UninitializedItemList(list, line!(), file!())
//     }
// }
