//! Stream server adapter; all command behavior comes from MockStream.

use super::{MockConfig, MockState, MockStream};
use std::io::{self, Read, Write};

/// Serve one connection until EOF or a configured disconnect fault.
///
/// The caller controls transport timeouts, listener lifecycle and concurrency.
/// Each call creates a fresh device. Response fragmentation follows mock read
/// chunk size, although TCP may coalesce writes on the network.
///
/// Returns final device state for assertions; I/O errors are propagated.
pub fn serve_connection(
    mut stream: impl Read + Write,
    config: MockConfig,
) -> io::Result<MockState> {
    let mut device = MockStream::new(config)?;
    let mut bytes = [0; 4096];
    loop {
        while device.pending_bytes() != 0 {
            let count = device.read(&mut bytes)?;
            stream.write_all(&bytes[..count])?;
        }
        stream.flush()?;
        if !device.is_connected() {
            return Ok(device.state().clone());
        }
        let count = match stream.read(&mut bytes) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(device.state().clone());
        }
        let mut consumed = 0;
        while consumed < count && device.is_connected() {
            consumed += device.write(&bytes[consumed..count])?;
        }
    }
}
