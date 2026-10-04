//! Buffered mixed text/binary shell transport over any `Read + Write` stream.
//!
//! Prompt matching spans reads and retains bytes after the prompt. Raw frames
//! are consumed by exact record count before text prompt matching resumes.
//! Read timeouts are supplied by the underlying stream and remain errors.

use crate::protocol::{ProtocolError, ScanRequest};
use std::{
    io::{self, BufReader, Read, Write},
    thread,
    time::Duration,
};

/// Console prompt outside binary frames.
const PROMPT: &[u8] = b"ch>";

/// Transport errors preserve their I/O or protocol source.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Read/write/flush failed, including timeout or disconnect.
    #[error("serial transport: {0}")]
    Io(#[from] io::Error),
    /// A response exceeded its configured memory bound.
    #[error("serial response exceeds {0} bytes")]
    ResponseTooLong(usize),
    /// A command contains a terminator or is empty/non-ASCII.
    #[error("a shell command must be one nonempty printable ASCII line")]
    InvalidCommand,
    /// A checked command returned a recognizable rejection or usage response.
    #[error("device rejected {command:?}: {response}")]
    CommandRejected {
        /// Command that was sent.
        command: String,
        /// Bounded text response from the device.
        response: String,
    },
    /// An invalid scan request was rejected before reading.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// A single buffered connection; do not bypass it to read from the same stream.
pub struct Shell<T> {
    /// Persistent buffering retains bytes beyond a prompt or raw frame.
    stream: BufReader<T>,
    /// Upper bound for text and pre-frame echo, independent of raw sample size.
    max_response_bytes: usize,
}

impl<T: Read + Write> Shell<T> {
    /// Wrap a serial stream or a deterministic in-memory transcript.
    pub fn new(stream: T, max_response_bytes: usize) -> Self {
        Self {
            stream: BufReader::new(stream),
            max_response_bytes,
        }
    }

    /// Write a complete shell command and carriage return, including partial writes.
    pub fn send(&mut self, command: &str) -> Result<(), TransportError> {
        if command.is_empty() || !command.bytes().all(|b| (b' '..=b'~').contains(&b)) {
            return Err(TransportError::InvalidCommand);
        }
        self.stream.get_mut().write_all(command.as_bytes())?;
        self.stream.get_mut().write_all(b"\r")?;
        self.stream.get_mut().flush()?;
        Ok(())
    }

    /// Read one prompt-terminated text response, excluding the prompt itself.
    ///
    /// A timeout or EOF before the complete prompt is an error, never success
    /// with partial text. Bytes after the prompt remain buffered for the next call.
    pub fn read_text(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut response = Vec::new();
        loop {
            let mut byte = [0];
            self.stream.read_exact(&mut byte)?;
            response.push(byte[0]);
            if response.ends_with(PROMPT) {
                response.truncate(response.len() - PROMPT.len());
                return Ok(response);
            }
            if response.len() >= self.max_response_bytes {
                return Err(TransportError::ResponseTooLong(self.max_response_bytes));
            }
        }
    }

    /// Drain an optional stale response before the first request.
    ///
    /// Idle timeout is expected here only. Disconnects and oversized responses
    /// remain errors. No subsequent command uses this relaxed timeout behavior.
    pub fn synchronize(&mut self) -> Result<(), TransportError> {
        match self.read_text() {
            Ok(_) => Ok(()),
            Err(TransportError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Send and consume one text command with configurable compatibility delays.
    pub fn command(
        &mut self,
        command: &str,
        delay: Duration,
        settle: Duration,
    ) -> Result<String, TransportError> {
        self.send(command)?;
        thread::sleep(delay);
        let response = self.read_text()?;
        thread::sleep(settle);
        Ok(String::from_utf8_lossy(&response).into_owned())
    }

    /// Send a mutating command, rejecting recognizable error/usage responses.
    ///
    /// This handles common shell responses, not every firmware-specific status.
    /// Use `command` for informational queries that intentionally return usage.
    pub fn command_checked(
        &mut self,
        command: &str,
        delay: Duration,
        settle: Duration,
    ) -> Result<String, TransportError> {
        let response = self.command(command, delay, settle)?;
        if response.lines().any(|line| {
            let line = line.trim_start().to_ascii_lowercase();
            line.starts_with("error:")
                || line.starts_with("usage:")
                || line.starts_with("unknown command")
        }) {
            return Err(TransportError::CommandRejected {
                command: command.into(),
                response,
            });
        }
        Ok(response)
    }

    /// Consume echo, an exact raw body, then its trailing shell prompt.
    ///
    /// Progress receives `(received_body_bytes, expected_body_bytes)`. A short
    /// frame, missing closing brace or absent prompt fails rather than returning
    /// a successful partial acquisition. No text token is searched inside samples.
    pub fn read_frame(
        &mut self,
        request: &ScanRequest,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<u8>, TransportError> {
        let expected = request.frame_len()?;
        let mut byte = [0];
        let mut prefix_len = 0;
        loop {
            self.stream.read_exact(&mut byte)?;
            if byte[0] == b'{' {
                break;
            }
            prefix_len += 1;
            if prefix_len >= self.max_response_bytes {
                return Err(TransportError::ResponseTooLong(self.max_response_bytes));
            }
        }
        let mut frame = vec![0; expected];
        frame[0] = b'{';
        let mut received = 1;
        for chunk in frame[1..].chunks_mut(4096) {
            self.stream.read_exact(chunk)?;
            received += chunk.len();
            progress(received, expected);
        }
        if frame.last() != Some(&b'}') {
            return Err(ProtocolError::MissingEnd.into());
        }
        self.read_text()?;
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Simulate arbitrary read fragmentation and short writes without hardware.
    struct Duplex {
        input: Cursor<Vec<u8>>,
        written: Vec<u8>,
        chunk: usize,
    }
    impl Read for Duplex {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let limit = buffer.len().min(self.chunk);
            self.input.read(&mut buffer[..limit])
        }
    }
    impl Write for Duplex {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = bytes.len().min(2);
            self.written.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn shell(bytes: &[u8], chunk: usize) -> Shell<Duplex> {
        Shell::new(
            Duplex {
                input: Cursor::new(bytes.to_vec()),
                written: Vec::new(),
                chunk,
            },
            256,
        )
    }

    #[test]
    fn prompts_span_reads_and_preserve_following_bytes() {
        for chunk in 1..=20 {
            let mut shell = shell(b"firstch>secondch>", chunk);
            assert_eq!(shell.read_text().unwrap(), b"first");
            assert_eq!(shell.read_text().unwrap(), b"second");
        }
    }

    #[test]
    fn commands_use_complete_writes_and_reject_injected_lines() {
        let mut shell = shell(b"replych>", 1);
        assert_eq!(
            shell
                .command("version", Duration::ZERO, Duration::ZERO)
                .unwrap(),
            "reply"
        );
        assert_eq!(shell.stream.get_ref().written, b"version\r");
        for command in ["", "scan\rresume", "scan\n", "\0"] {
            assert!(shell.send(command).is_err());
        }
        assert_eq!(shell.stream.get_ref().written, b"version\r");
    }

    #[test]
    fn frame_is_length_delimited_and_next_response_is_retained() {
        let request = ScanRequest {
            start_hz: 0,
            stop_hz: 10,
            points: 2,
            repeat: 1,
        };
        for chunk in 1..=32 {
            let mut shell = shell(b"scanraw echo\r\n{x{}xch}\r\nch>nextch>", chunk);
            let mut last = (0, 0);
            let frame = shell
                .read_frame(&request, |received, total| last = (received, total))
                .unwrap();
            assert_eq!(frame, b"{x{}xch}");
            assert_eq!(last, (8, 8));
            assert_eq!(shell.read_text().unwrap(), b"next");
        }
    }

    #[test]
    fn eof_truncation_and_size_limits_are_errors() {
        assert!(shell(b"missing prompt", 5).read_text().is_err());
        assert!(shell(&[b'a'; 257], 32).read_text().is_err());
        let request = ScanRequest {
            start_hz: 0,
            stop_hz: 10,
            points: 2,
            repeat: 1,
        };
        for input in [b"{x12}".as_slice(), b"{x12x34}", b"{x12x34!ch>"] {
            assert!(shell(input, 1).read_frame(&request, |_, _| {}).is_err());
        }
    }

    #[test]
    fn timeout_is_only_tolerated_during_initial_synchronization() {
        struct Timeout;
        impl Read for Timeout {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::TimedOut.into())
            }
        }
        impl Write for Timeout {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut shell = Shell::new(Timeout, 256);
        assert!(shell.synchronize().is_ok());
        assert!(shell.read_text().is_err());
    }
}
