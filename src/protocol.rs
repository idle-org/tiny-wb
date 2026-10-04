//! Basic input selection and pure, length-checked `scanraw` decoding.
//!
//! Wire invariants live here: delimiters, little-endian 16-bit samples and
//! 1/32 dB scaling. No serial port, clock, file or terminal is accessed.

use crate::config::FrequencyAxis;

/// Highest supported Basic frequency, in Hz. Ultra is not negotiated yet.
pub const BASIC_MAX_HZ: u64 = 960_000_000;
/// Host allocation limit retained from the original UI.
pub const MAX_POINTS: usize = 1_000_001;

/// One decoded observation; CSV and UI both consume these physical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Frequency assigned by the selected host labeling convention, in MHz.
    pub frequency_mhz: f64,
    /// Level after subtracting the configured raw offset, in dBm.
    pub level_dbm: f64,
}

/// Validatable acquisition parameters, independent of UI widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanRequest {
    /// First requested frequency, in Hz.
    pub start_hz: u64,
    /// Requested upper boundary, in Hz.
    pub stop_hz: u64,
    /// Number of raw sample records.
    pub points: usize,
    /// Measurements per frequency (1–1,000).
    pub repeat: usize,
}

/// The Basic's physical input path; this is not an Ultra capability model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Input path below 350 MHz.
    Low,
    /// High path, also used for a span starting at/above 240 MHz and crossing 350 MHz.
    High,
}

impl InputMode {
    /// Keyword used by the `mode ... input` shell command.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }
}

/// Validation and frame errors, with bounded diagnostics rather than raw dumps.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// Invalid acquisition parameters or unsupported input span.
    #[error("invalid scan request: {0}")]
    InvalidRequest(String),
    /// The supplied frame does not begin with `{`.
    #[error("raw scan is missing its opening brace")]
    MissingStart,
    /// The frame length differs from the requested number of records.
    #[error("raw frame has {actual} bytes; expected {expected}")]
    Length {
        /// Expected body length, including braces.
        expected: usize,
        /// Actual body length, including braces.
        actual: usize,
    },
    /// A record does not begin with the alignment byte `x`.
    #[error("sample {index} has alignment byte 0x{found:02x}; expected 0x78")]
    Alignment {
        /// Zero-based sample index.
        index: usize,
        /// Received alignment byte.
        found: u8,
    },
    /// No closing brace occurs at the exact end of the requested records.
    #[error("raw scan is missing its closing brace")]
    MissingEnd,
}

impl ScanRequest {
    /// Validate point/repeat bounds and choose the historical Basic input path.
    pub fn input_mode(&self) -> Result<InputMode, ProtocolError> {
        if !(2..=MAX_POINTS).contains(&self.points) || !(1..=1000).contains(&self.repeat) {
            return Err(ProtocolError::InvalidRequest(
                "points must be 2..=1000001 and repeat 1..=1000".into(),
            ));
        }
        if self.start_hz > self.stop_hz || self.stop_hz > BASIC_MAX_HZ {
            return Err(ProtocolError::InvalidRequest(
                "frequencies must be ordered within 0–960 MHz".into(),
            ));
        }
        if self.stop_hz < 350_000_000 {
            Ok(InputMode::Low)
        } else if self.start_hz >= 240_000_000 {
            Ok(InputMode::High)
        } else {
            Err(ProtocolError::InvalidRequest(
                "a span crossing 350 MHz must start at or above 240 MHz".into(),
            ))
        }
    }

    /// Exact framed byte count; validation happens before multiplication/allocation.
    pub fn frame_len(&self) -> Result<usize, ProtocolError> {
        self.input_mode()?;
        Ok(self.points * 3 + 2)
    }
}

/// Decode exactly one raw body, including braces and excluding echo/prompt.
///
/// `zero_offset` must match the `zero` command sent to the device. Endpoint
/// labeling is explicit because the historical host and reviewed firmware use
/// different denominators. Malformed/short/extra records are rejected before
/// callers can export a completed scan.
pub fn decode_frame(
    frame: &[u8],
    request: &ScanRequest,
    zero_offset: u16,
    axis: FrequencyAxis,
) -> Result<Vec<Sample>, ProtocolError> {
    let expected = request.frame_len()?;
    if zero_offset > 2047 {
        return Err(ProtocolError::InvalidRequest(
            "zero offset must be 0..=2047".into(),
        ));
    }
    if frame.first() != Some(&b'{') {
        return Err(ProtocolError::MissingStart);
    }
    if frame.len() != expected {
        return Err(ProtocolError::Length {
            expected,
            actual: frame.len(),
        });
    }
    if frame.last() != Some(&b'}') {
        return Err(ProtocolError::MissingEnd);
    }
    let intervals = match axis {
        FrequencyAxis::Inclusive => request.points - 1,
        FrequencyAxis::Firmware => request.points,
    };
    let step = (request.stop_hz - request.start_hz) as f64 / intervals as f64;
    frame[1..frame.len() - 1]
        .chunks_exact(3)
        .enumerate()
        .map(|(index, record)| {
            if record[0] != b'x' {
                return Err(ProtocolError::Alignment {
                    index,
                    found: record[0],
                });
            }
            Ok(Sample {
                frequency_mhz: (request.start_hz as f64 + index as f64 * step) / 1_000_000.0,
                level_dbm: f64::from(u16::from_le_bytes([record[1], record[2]])) / 32.0
                    - f64::from(zero_offset),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ScanRequest {
        ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 960_000_000,
            points: 2,
            repeat: 1,
        }
    }

    #[test]
    fn basic_modes_and_boundaries() {
        for (start_hz, stop_hz, expected) in [
            (0, 349_999_999, Some(InputMode::Low)),
            (0, 350_000_000, None),
            (240_000_000, 350_000_000, Some(InputMode::High)),
            (350_000_000, 960_000_000, Some(InputMode::High)),
            (960_000_000, 960_000_000, Some(InputMode::High)),
            (960_000_000, 960_000_001, None),
            (100, 99, None),
        ] {
            assert_eq!(
                ScanRequest {
                    start_hz,
                    stop_hz,
                    ..request()
                }
                .input_mode()
                .ok(),
                expected
            );
        }
        for points in [0, 1, MAX_POINTS + 1, usize::MAX] {
            assert!(ScanRequest {
                points,
                ..request()
            }
            .frame_len()
            .is_err());
        }
        assert!(ScanRequest {
            repeat: 0,
            ..request()
        }
        .input_mode()
        .is_err());
        assert!(ScanRequest {
            repeat: 1001,
            ..request()
        }
        .input_mode()
        .is_err());
    }

    #[test]
    fn little_endian_levels_offsets_and_both_axes() {
        let frame = b"{x\x00\x08x\x00\x10}";
        let inclusive = decode_frame(frame, &request(), 128, FrequencyAxis::Inclusive).unwrap();
        assert_eq!(
            inclusive,
            vec![
                Sample {
                    frequency_mhz: 240.0,
                    level_dbm: -64.0
                },
                Sample {
                    frequency_mhz: 960.0,
                    level_dbm: 0.0
                }
            ]
        );
        let firmware = decode_frame(frame, &request(), 174, FrequencyAxis::Firmware).unwrap();
        assert_eq!(
            firmware[1],
            Sample {
                frequency_mhz: 600.0,
                level_dbm: -46.0
            }
        );
    }

    #[test]
    fn truncated_extra_and_misaligned_frames_are_rejected_without_panics() {
        let frame = b"{x\x00\x08x\x00\x10}";
        for length in 0..frame.len() {
            assert!(
                decode_frame(&frame[..length], &request(), 128, FrequencyAxis::Inclusive).is_err()
            );
        }
        for frame in [
            b"{x\x00\x08x\x00\x10}extra".as_slice(),
            b"{y\x00\x08x\x00\x10}",
            b"{x\x00\x08x\x00\x10!",
            b"{x\x00\x08}       ",
        ] {
            assert!(decode_frame(frame, &request(), 128, FrequencyAxis::Inclusive).is_err());
        }
    }

    #[test]
    fn binary_delimiters_are_sample_data() {
        let samples = decode_frame(b"{x{}xch}", &request(), 128, FrequencyAxis::Inclusive).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(
            samples[0].level_dbm,
            f64::from(u16::from_le_bytes(*b"{}")) / 32.0 - 128.0
        );
    }
}
