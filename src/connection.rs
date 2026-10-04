//! Optional physical serial and portable TCP adapters for the same shell codec.
//!
//! Integrations may instead supply their own `Read + Write` stream directly to
//! [`crate::transport::Shell`]. TCP means an explicit bridge or mock server,
//! not native network support on the instrument.

use crate::config::{Backend, Config, ConfigError};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::Duration,
};

/// An error opening a configured connection, before any command is sent.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    /// Settings failed validation.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The TCP connection or its timeout configuration failed.
    #[error("TCP connection: {0}")]
    Tcp(#[from] io::Error),
    /// Native serial opening failed.
    #[cfg(feature = "serial")]
    #[error("serial connection: {0}")]
    Serial(#[from] serialport::Error),
    /// Native serial support was omitted from this library build.
    #[error("enable the serial Cargo feature to open a USB/serial device")]
    SerialFeatureDisabled,
}

/// A runtime-selected stream; the protocol layer only sees `Read + Write`.
pub enum Connection {
    /// TCP socket with configured operation timeouts.
    Tcp(TcpStream),
    /// Native serial port with a configured read timeout.
    #[cfg(feature = "serial")]
    Serial(Box<dyn serialport::SerialPort>),
}

/// Validate settings and open the selected backend without sending commands.
///
/// TCP uses numeric addresses and bounded connect/read/write operations. Serial
/// is optional so library consumers do not require system libudev by default
/// when using `default-features = false`.
pub fn open_connection(config: &Config) -> Result<Connection, ConnectionError> {
    config.validate()?;
    match config.connection.backend {
        Backend::Tcp => {
            let address = config
                .connection
                .tcp_address
                .parse()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            let timeout = Duration::from_millis(config.connection.tcp_timeout_ms);
            let stream = TcpStream::connect_timeout(&address, timeout)?;
            stream.set_read_timeout(Some(timeout))?;
            stream.set_write_timeout(Some(timeout))?;
            stream.set_nodelay(true)?;
            Ok(Connection::Tcp(stream))
        }
        Backend::Serial => {
            #[cfg(feature = "serial")]
            {
                Ok(Connection::Serial(
                    serialport::new(&config.port, config.baudrate)
                        .timeout(Duration::from_millis(config.serial.timeout_ms))
                        .open()?,
                ))
            }
            #[cfg(not(feature = "serial"))]
            {
                Err(ConnectionError::SerialFeatureDisabled)
            }
        }
    }
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buffer),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => stream.read(buffer),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write(bytes),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => stream.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush(),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => stream.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_configuration_is_validated_before_connecting() {
        for input in [
            "[connection]\nbackend='udp'",
            "[connection]\ntcp_address='localhost:7355'",
            "[connection]\ntcp_address='127.0.0.1:0'",
            "[connection]\ntcp_timeout_ms=0",
        ] {
            assert!(Config::from_toml(input).is_err());
        }
        let mut config = Config::default();
        config.connection.tcp_address = "invalid".into();
        assert!(matches!(
            open_connection(&config),
            Err(ConnectionError::Config(_))
        ));
    }

    #[cfg(not(feature = "serial"))]
    #[test]
    fn native_serial_requires_explicit_feature() {
        assert!(matches!(
            open_connection(&Config::default()),
            Err(ConnectionError::SerialFeatureDisabled)
        ));
    }
}
