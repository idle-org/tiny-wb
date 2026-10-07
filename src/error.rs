//! Application boundary errors; core modules keep their own typed errors.

/// Errors surfaced during startup or acquisition.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    /// Command-line arguments do not select a supported startup mode.
    #[error("{0}")]
    Usage(String),
    /// Configuration read, validation or persistence failed.
    #[error(transparent)]
    Config(#[from] tiny_wb::config::ConfigError),
    /// Serial enumeration or opening failed.
    #[error("serial connection: {0}")]
    Port(#[from] serialport::Error),
    /// Configured serial/TCP connection failed before acquisition.
    #[error(transparent)]
    Connection(#[from] tiny_wb::connection::ConnectionError),
    /// Filesystem or terminal I/O failed.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A requested scan CSV could not be opened or decoded.
    #[error("cannot load CSV {}: {source}", path.display())]
    Csv {
        /// Requested input path.
        path: std::path::PathBuf,
        /// File or data error.
        source: std::io::Error,
    },
    /// Acquisition failed, potentially including a failed resume attempt.
    #[error(transparent)]
    Acquisition(#[from] tiny_wb::acquisition::AcquisitionError),
    /// The file logger could not be initialized.
    #[error("logging: {0}")]
    Logger(#[from] flexi_logger::FlexiLoggerError),
    /// Error reporting or the terminal event loop failed.
    #[error("terminal: {0}")]
    Terminal(#[from] color_eyre::Report),
}

/// Errors from UI selection and transitions; rendering displays their context.
#[derive(Debug, thiserror::Error)]
pub(crate) enum UiError {
    /// A selection index no longer belongs to its list.
    #[error("selected menu item is unavailable")]
    MenuSelection,
    /// An internal widget was used before initialization.
    #[error("widget {0} is not initialized ({2}:{1})")]
    UninitializedItemList(&'static str, u32, &'static str),
    /// No serial device is available or selected.
    #[error("no serial port is selected; connect the instrument and restart")]
    NoPortSelected,
    /// Configuration or acquisition preparation failed.
    #[error(transparent)]
    Application(#[from] AppError),
    /// Completed scan data was already consumed or never supplied.
    #[error("the completed scan has no available samples")]
    NoReturnData,
}
