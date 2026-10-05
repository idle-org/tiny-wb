//! Model-independent request validation and length-checked `scanraw` decoding.
//!
//! Wire invariants live here: delimiters, little-endian 16-bit samples and
//! 1/32 dB scaling. No serial port, clock, file or terminal is accessed.

use crate::config::FrequencyAxis;

/// Highest supported Basic frequency, in Hz.
pub const BASIC_MAX_HZ: u64 = 960_000_000;
/// Native `scanraw` level bias used by tinySA Basic firmware, in dB.
pub const BASIC_RAW_LEVEL_OFFSET_DB: u16 = 128;
/// Native `scanraw` level bias used by tinySA Ultra firmware, in dB.
pub const ULTRA_RAW_LEVEL_OFFSET_DB: u16 = 174;
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

/// Legacy Basic input-path selection; use `DeviceModel::plan` for automatic models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Basic low input.
    Low,
    /// Basic high input.
    High,
}

impl InputMode {
    /// Console keyword identifying the Basic input path.
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
    /// Validate this request using the legacy Basic profile.
    /// Ultra callers should use `DeviceModel::Ultra.plan` instead.
    pub fn input_mode(&self) -> Result<InputMode, ProtocolError> {
        let plan = crate::device::DeviceModel::Basic.plan(self)?;
        Ok(if plan.input_command == "mode high input" {
            InputMode::High
        } else {
            InputMode::Low
        })
    }
    /// Validate model-independent ordering and point/repeat bounds.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !(2..=MAX_POINTS).contains(&self.points) || !(1..=1000).contains(&self.repeat) {
            return Err(ProtocolError::InvalidRequest(
                "points must be 2..=1000001 and repeat 1..=1000".into(),
            ));
        }
        if self.start_hz > self.stop_hz {
            return Err(ProtocolError::InvalidRequest(
                "start frequency must not exceed stop frequency".into(),
            ));
        }
        Ok(())
    }

    /// Exact framed byte count; validation happens before multiplication/allocation.
    pub fn frame_len(&self) -> Result<usize, ProtocolError> {
        self.validate()?;
        Ok(self.points * 3 + 2)
    }
}

/// Decode exactly one raw body, including braces and excluding echo/prompt.
///
/// `zero_offset` must match the `zero` command sent to the device. Frequency
/// labeling is explicit because legacy tiny-wb output and `scanraw` use
/// different arithmetic. Malformed/short/extra records are rejected before
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
    // The exact frame length checked above guarantees no partial record remains.
    frame[1..frame.len() - 1]
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(index, record)| {
            if record[0] != b'x' {
                return Err(ProtocolError::Alignment {
                    index,
                    found: record[0],
                });
            }
            Ok(Sample {
                frequency_mhz: match axis {
                    FrequencyAxis::Inclusive => {
                        let intervals = request.points - 1;
                        (request.start_hz as f64
                            + index as f64 * (request.stop_hz - request.start_hz) as f64
                                / intervals as f64)
                            / 1_000_000.0
                    }
                    FrequencyAxis::Firmware => {
                        firmware_frequency_hz(request, index) as f64 / 1_000_000.0
                    }
                },
                level_dbm: f64::from(u16::from_le_bytes([record[1], record[2]])) / 32.0
                    - f64::from(zero_offset),
            })
        })
        .collect()
}

/// Reproduce the reviewed firmware's `scanraw` frequency expression.
///
/// The C implementation first performs integer division, converts that result
/// to a 32-bit `float`, multiplies it by the point index, then truncates the
/// product back to `freq_t` before adding `start`.
pub(crate) fn firmware_frequency_hz(request: &ScanRequest, index: usize) -> u64 {
    debug_assert!(index < request.points);
    let integer_step_hz = (request.stop_hz - request.start_hz) / request.points as u64;
    let firmware_step_hz = integer_step_hz as f32;
    request.start_hz + (firmware_step_hz * index as f32) as u64
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
    fn structural_request_boundaries_are_model_independent() {
        assert!(ScanRequest {
            start_hz: 100,
            stop_hz: 99,
            ..request()
        }
        .validate()
        .is_err());
        assert!(ScanRequest {
            start_hz: 0,
            stop_hz: u64::MAX,
            ..request()
        }
        .validate()
        .is_ok());
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
        .validate()
        .is_err());
        assert!(ScanRequest {
            repeat: 1001,
            ..request()
        }
        .validate()
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
    fn firmware_axis_reproduces_integer_step_and_float_rounding() {
        let request = ScanRequest {
            start_hz: 0,
            stop_hz: 10,
            points: 3,
            repeat: 1,
        };
        let samples =
            decode_frame(b"{x\0\0x\0\0x\0\0}", &request, 0, FrequencyAxis::Firmware).unwrap();
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.frequency_mhz * 1_000_000.0)
                .collect::<Vec<_>>(),
            vec![0.0, 3.0, 6.0]
        );

        let request = ScanRequest {
            start_hz: 0,
            stop_hz: 100_000_002,
            points: 2,
            repeat: 1,
        };
        assert_eq!(firmware_frequency_hz(&request, 1), 50_000_000);
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
