//! Bounded, stateful console simulation and an in-memory duplex stream.

use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
};

/// Synthetic identity and command differences, not hardware certification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MockModel {
    /// Basic low/high input syntax; initial raw offset 128 dB.
    Basic,
    /// Ultra input syntax, LNA/Ultra toggles; initial raw offset 174 dB.
    Ultra,
}

/// One-shot fault applied to the first matching command in each new session.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MockFault {
    /// Normal behavior.
    #[default]
    None,
    /// Reject the command without changing device state.
    Reject,
    /// Drop the command and its response, making the next empty read time out.
    Timeout,
    /// Close the stream immediately without processing the command.
    Disconnect,
    /// Omit the trailing prompt after executing the command.
    MissingPrompt,
    /// Remove the end of a scanraw frame and its prompt.
    TruncatedFrame,
    /// Replace the first scanraw record's alignment byte.
    BadAlignment,
}

/// Simulator policy, loadable from TOML independently of application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MockConfig {
    /// Synthetic device identity.
    pub model: MockModel,
    /// Echo complete commands before responding.
    pub echo: bool,
    /// Maximum bytes returned per in-memory read (also server write chunk size).
    pub read_chunk_bytes: usize,
    /// Maximum bytes accepted per in-memory write, exercising complete writes.
    pub write_chunk_bytes: usize,
    /// TCP example read/write timeout, in milliseconds (1 through 600,000).
    pub session_timeout_ms: u64,
    /// Maximum command line size, before a terminator.
    pub command_limit_bytes: usize,
    /// Maximum requested scan points, bounding frame allocation.
    pub max_points: usize,
    /// Synthetic Ultra ceiling in Hz; not a hardware capability claim.
    pub ultra_max_hz: u64,
    /// Deterministic spectrum floor, in dBm.
    pub floor_dbm: f64,
    /// Triangular peak level, in dBm.
    pub peak_dbm: f64,
    /// Synthetic carrier center, in Hz.
    pub peak_frequency_hz: u64,
    /// Half-width of the triangular peak, in Hz.
    pub peak_width_hz: u64,
    /// One-shot response fault.
    pub fault: MockFault,
    /// Command verb on which to inject the fault, for example `scanraw`.
    pub fault_command: String,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            model: MockModel::Basic,
            echo: true,
            read_chunk_bytes: 4096,
            write_chunk_bytes: 4096,
            session_timeout_ms: 30_000,
            command_limit_bytes: 256,
            max_points: 10001,
            ultra_max_hz: 6_000_000_000,
            floor_dbm: -96.0,
            peak_dbm: -32.0,
            peak_frequency_hz: 300_000_000,
            peak_width_hz: 10_000_000,
            fault: MockFault::None,
            fault_command: "scanraw".into(),
        }
    }
}

impl MockConfig {
    /// Reject unbounded allocations, non-finite signal values and invalid fault settings.
    pub fn validate(&self) -> io::Result<()> {
        let valid = (1..=600_000).contains(&self.session_timeout_ms)
            && (1..=65536).contains(&self.read_chunk_bytes)
            && (1..=65536).contains(&self.write_chunk_bytes)
            && (16..=4096).contains(&self.command_limit_bytes)
            && (2..=1_000_001).contains(&self.max_points)
            && (960_000_000..=12_000_000_000).contains(&self.ultra_max_hz)
            && self.floor_dbm.is_finite()
            && self.peak_dbm.is_finite()
            && (-174.0..=100.0).contains(&self.floor_dbm)
            && (self.floor_dbm..=100.0).contains(&self.peak_dbm)
            && self.peak_frequency_hz <= 12_000_000_000
            && (1..=12_000_000_000).contains(&self.peak_width_hz)
            && !self.fault_command.is_empty()
            && self
                .fault_command
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_')
            && (!matches!(
                self.fault,
                MockFault::TruncatedFrame | MockFault::BadAlignment
            ) || self.fault_command == "scanraw");
        if valid {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid mock configuration bounds or fault/command combination",
            ))
        }
    }
}

/// Observable simulated state. No field represents a physical measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct MockState {
    /// Synthetic model.
    pub model: MockModel,
    /// Whether normal sweeping is paused.
    pub paused: bool,
    /// Last selected input keyword (`low`, `high`, or `input`).
    pub input: String,
    /// Display sweep start, in Hz.
    pub start_hz: u64,
    /// Display sweep stop, in Hz.
    pub stop_hz: u64,
    /// Last requested raw scan sample count.
    pub points: usize,
    /// Measurement repeat count.
    pub repeat: usize,
    /// Offset used by raw fixed-point encoding, in dB.
    pub zero_offset: u16,
    /// Simulated LNA state (Ultra only; no RF effect).
    pub lna: bool,
    /// Simulated Ultra state (no RF effect).
    pub ultra: bool,
    /// Last 1,024 complete commands, oldest first; excludes overlong lines.
    pub commands: VecDeque<String>,
}

/// A deterministic `Read + Write` fake instrument with partial I/O support.
///
/// Empty reads return `TimedOut`, not EOF; a disconnect fault returns EOF and
/// rejects later writes. Responses are generated only when commands arrive.
/// Use `Shell::new(&mut mock, limit)` to inspect state after dropping the shell.
pub struct MockStream {
    config: MockConfig,
    state: MockState,
    input: Vec<u8>,
    output: VecDeque<u8>,
    discarding: bool,
    connected: bool,
    fault_used: bool,
}

impl MockStream {
    /// Construct a fresh session with an initial shell prompt.
    pub fn new(config: MockConfig) -> io::Result<Self> {
        config.validate()?;
        let state = MockState {
            model: config.model,
            paused: false,
            input: if config.model == MockModel::Basic {
                "low"
            } else {
                "input"
            }
            .into(),
            start_hz: 240_000_000,
            stop_hz: 960_000_000,
            points: 101.min(config.max_points),
            repeat: 1,
            zero_offset: if config.model == MockModel::Basic {
                128
            } else {
                174
            },
            lna: false,
            ultra: false,
            commands: VecDeque::new(),
        };
        Ok(Self {
            config,
            state,
            input: Vec::new(),
            output: VecDeque::from(b"ch>".to_vec()),
            discarding: false,
            connected: true,
            fault_used: false,
        })
    }

    /// Read-only state and bounded command history, for assertions.
    pub fn state(&self) -> &MockState {
        &self.state
    }
    /// Whether a disconnect fault closed the simulated connection.
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    /// Number of currently queued response bytes; used by the TCP adapter.
    pub fn pending_bytes(&self) -> usize {
        self.output.len()
    }

    /// Bound pipelined responses as well as individual scan allocations.
    fn enqueue(&mut self, bytes: &[u8]) -> io::Result<()> {
        let limit = self.config.max_points * 3 + self.config.command_limit_bytes + 65536;
        if self.output.len().saturating_add(bytes.len()) > limit {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "mock response queue limit exceeded; read responses before pipelining commands",
            ));
        }
        self.output.extend(bytes);
        Ok(())
    }

    fn execute(&mut self, command: &str) -> io::Result<()> {
        if self.state.commands.len() == 1024 {
            self.state.commands.pop_front();
        }
        self.state.commands.push_back(command.into());
        let verb = command.split_whitespace().next().unwrap_or("");
        let fault = if !self.fault_used && verb == self.config.fault_command {
            self.fault_used = true;
            self.config.fault
        } else {
            MockFault::None
        };
        match fault {
            MockFault::Timeout => return Ok(()),
            MockFault::Disconnect => {
                self.connected = false;
                self.output.clear();
                return Ok(());
            }
            _ => {}
        }
        let mut response = if self.config.echo {
            format!("{command}\r\n").into_bytes()
        } else {
            Vec::new()
        };
        let mut body = if fault == MockFault::Reject {
            b"error: injected rejection\r\n".to_vec()
        } else {
            self.respond(command)
        };
        if fault == MockFault::BadAlignment && body.first() == Some(&b'{') {
            body[1] = b'!';
        }
        if fault == MockFault::TruncatedFrame && body.first() == Some(&b'{') {
            body.truncate(body.len().saturating_sub(4));
        }
        response.extend(body);
        if !matches!(fault, MockFault::MissingPrompt | MockFault::TruncatedFrame) {
            response.extend_from_slice(b"ch>");
        }
        self.enqueue(&response)
    }

    /// Unsupported commands fail explicitly rather than inventing successful firmware behavior.
    fn respond(&mut self, command: &str) -> Vec<u8> {
        let fields: Vec<_> = command.split_whitespace().collect();
        let ok = b"".to_vec();
        let invalid = || b"error: unsupported command or invalid arguments in mock\r\n".to_vec();
        match fields.as_slice() {
            [] => ok,
            ["version"] => {
                format!("tiny-wb mock {:?} 0.1 (synthetic)\r\n", self.state.model).into_bytes()
            }
            ["info"] => format!(
                "tinySA {} mock; not real firmware\r\n",
                if self.state.model == MockModel::Basic {
                    "Basic"
                } else {
                    "Ultra"
                }
            )
            .into_bytes(),
            ["help"] => {
                let extra = if self.state.model == MockModel::Ultra {
                    " lna ultra"
                } else {
                    ""
                };
                format!("Commands: version info help pause resume mode repeat zero sweep scan scanraw leveloffset vbat usart_cfg threads marker{extra}\r\n").into_bytes()
            }
            ["pause"] => {
                self.state.paused = true;
                ok
            }
            ["resume"] => {
                self.state.paused = false;
                ok
            }
            ["mode", "low" | "high", "input"] if self.state.model == MockModel::Basic => {
                self.state.input = fields[1].into();
                ok
            }
            ["mode", "input"] | ["mode", "low", "input"]
                if self.state.model == MockModel::Ultra =>
            {
                self.state.input = "input".into();
                ok
            }
            ["lna", value] | ["ultra", value]
                if self.state.model == MockModel::Ultra && matches!(*value, "on" | "off") =>
            {
                if fields[0] == "lna" {
                    self.state.lna = *value == "on";
                } else {
                    self.state.ultra = *value == "on";
                }
                ok
            }
            ["repeat", value] => match value.parse::<usize>() {
                Ok(value) if (1..=1000).contains(&value) => {
                    self.state.repeat = value;
                    ok
                }
                _ => invalid(),
            },
            ["zero", value] => match value.parse::<u16>() {
                Ok(value) if value <= 2047 => {
                    self.state.zero_offset = value;
                    ok
                }
                _ => invalid(),
            },
            ["sweep", "start" | "stop", value] => match value.parse::<u64>() {
                Ok(value) if value <= self.max_hz() => {
                    if fields[1] == "start" {
                        self.state.start_hz = value;
                    } else {
                        self.state.stop_hz = value;
                    }
                    ok
                }
                _ => invalid(),
            },
            ["sweep"] => format!(
                "{} {} {}\r\n",
                self.state.start_hz, self.state.stop_hz, self.state.points
            )
            .into_bytes(),
            ["scanraw", start, stop, points] => {
                if let Some((start, stop, points)) = self.scan_arguments(start, stop, points) {
                    self.state.points = points;
                    let mut frame = Vec::with_capacity(points * 3 + 4);
                    frame.push(b'{');
                    for index in 0..points {
                        // The mock uses the reviewed firmware convention, span / points.
                        let frequency =
                            start as f64 + (stop - start) as f64 * index as f64 / points as f64;
                        let strength = (1.0
                            - (frequency - self.config.peak_frequency_hz as f64).abs()
                                / self.config.peak_width_hz as f64)
                            .clamp(0.0, 1.0);
                        let level = self.config.floor_dbm
                            + strength * (self.config.peak_dbm - self.config.floor_dbm);
                        let raw = ((level + f64::from(self.state.zero_offset)) * 32.0)
                            .round()
                            .clamp(0.0, 65535.0) as u16;
                        frame.push(b'x');
                        frame.extend(raw.to_le_bytes());
                    }
                    frame.extend_from_slice(b"}\r\n");
                    frame
                } else {
                    invalid()
                }
            }
            // Compatibility-only preparation used by the current application. No trace is returned.
            ["scan", start, stop, "290", "101"]
                if self.scan_arguments(start, stop, "2").is_some() =>
            {
                ok
            }
            ["leveloffset"] => b"leveloffset mock 0\r\n".to_vec(),
            ["vbat"] => b"4000 mV (mock)\r\n".to_vec(),
            ["usart_cfg"] => b"115200 (mock)\r\n".to_vec(),
            ["threads"] => b"mock: synchronous\r\n".to_vec(),
            ["marker"] => b"1 0 300000000 -32 (mock)\r\n".to_vec(),
            _ => invalid(),
        }
    }

    fn max_hz(&self) -> u64 {
        if self.state.model == MockModel::Basic {
            960_000_000
        } else {
            self.config.ultra_max_hz
        }
    }

    fn scan_arguments(&self, start: &str, stop: &str, points: &str) -> Option<(u64, u64, usize)> {
        let (start, stop, points) = (
            start.parse::<u64>().ok()?,
            stop.parse::<u64>().ok()?,
            points.parse::<usize>().ok()?,
        );
        if start > stop || stop > self.max_hz() || !(2..=self.config.max_points).contains(&points) {
            return None;
        }
        if self.state.model == MockModel::Basic {
            let valid = match self.state.input.as_str() {
                "low" => stop < 350_000_000,
                "high" => start >= 240_000_000,
                _ => false,
            };
            if !valid {
                return None;
            }
        }
        Some((start, stop, points))
    }
}

impl Read for MockStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.output.is_empty() {
            return if self.connected {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(0)
            };
        }
        let count = buffer
            .len()
            .min(self.config.read_chunk_bytes)
            .min(self.output.len());
        for byte in &mut buffer[..count] {
            *byte = self.output.pop_front().expect("queue length checked");
        }
        Ok(count)
    }
}

impl Write for MockStream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if !self.connected {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let count = bytes.len().min(self.config.write_chunk_bytes);
        for (index, &byte) in bytes[..count].iter().enumerate() {
            if matches!(byte, b'\r' | b'\n') {
                if self.discarding {
                    self.discarding = false;
                    self.enqueue(b"error: command too long\r\nch>")?;
                } else if !self.input.is_empty() {
                    let input = std::mem::take(&mut self.input);
                    match String::from_utf8(input) {
                        Ok(command) if command.bytes().all(|c| (b' '..=b'~').contains(&c)) => {
                            self.execute(&command)?
                        }
                        _ => self.enqueue(b"error: command must be printable ASCII\r\nch>")?,
                    }
                }
            } else if !self.discarding {
                if self.input.len() == self.config.command_limit_bytes {
                    self.input.clear();
                    self.discarding = true;
                } else {
                    self.input.push(byte);
                }
            }
            if !self.connected {
                return Ok(index + 1);
            }
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
