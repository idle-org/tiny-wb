//! Device identity parsing and model-specific acquisition policy.
//!
//! Identification consumes explicit `version`/`info` evidence. This module
//! selects only acquisition behavior shared by the detected family; it does not
//! infer an Ultra hardware revision or unlock extended/harmonic modes.

use crate::protocol::{
    ProtocolError, ScanRequest, BASIC_MAX_HZ, BASIC_RAW_LEVEL_OFFSET_DB, ULTRA_RAW_LEVEL_OFFSET_DB,
};
use std::fmt;

/// Lowest documented Ultra analyzer frequency, in Hz.
pub const ULTRA_MIN_HZ: u64 = 100_000;
/// Common normal-mode ceiling across supported Ultra family revisions, in Hz.
pub const ULTRA_NORMAL_MAX_HZ: u64 = 800_000_000;

/// Device family established from console identity replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceModel {
    /// Original 2.8-inch tinySA with low and high input paths.
    Basic,
    /// tinySA Ultra/Ultra+ family, restricted here to the common normal range.
    Ultra,
}

impl fmt::Display for DeviceModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Basic => "tinySA Basic",
            Self::Ultra => "tinySA Ultra family",
        })
    }
}

/// Identity replies retained together with the detected family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Detected device family.
    pub model: DeviceModel,
    /// Verbatim prompt-stripped `version` response.
    pub version: String,
    /// Verbatim prompt-stripped `info` response.
    pub info: String,
}

/// A validated model-specific command and decoding plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquisitionPlan {
    /// Detected device family.
    pub model: DeviceModel,
    /// Model-correct console command selecting the input path.
    pub input_command: &'static str,
    /// Native raw-level encoding offset, in dB.
    pub native_zero_offset_db: u16,
}

/// Device identification failures occur before acquisition settings are changed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceError {
    /// Neither reply contained a recognized model signature.
    #[error("unsupported device identity; expected tinySA_v… or tinySA4_v… in version/info")]
    Unknown,
    /// Basic and Ultra signatures occurred in the same identity exchange.
    #[error("conflicting device identity: replies identify both tinySA Basic and Ultra")]
    Conflicting,
}

impl DeviceIdentity {
    /// Detect a Basic or Ultra family from `version` and `info` replies.
    ///
    /// Basic firmware uses a `tinySA_v…` version token; Ultra-family firmware
    /// uses `tinySA4_v…`. Explicit Basic/Ultra names in `info` are also accepted.
    /// Shared source URLs and generic `tinySA` substrings are not sufficient.
    pub fn detect(version: String, info: String) -> Result<Self, DeviceError> {
        let version_lower = version.to_ascii_lowercase();
        let info_lower = info.to_ascii_lowercase();
        let ultra = version_lower
            .split_whitespace()
            .any(|token| token.starts_with("tinysa4_v"))
            || info_lower.contains("tinysa ultra");
        let basic = version_lower
            .split_whitespace()
            .any(|token| token.starts_with("tinysa_v"))
            || info_lower.contains("tinysa basic")
            || info_lower.lines().any(|line| line.trim() == "tinysa");
        let model = match (basic, ultra) {
            (true, false) => DeviceModel::Basic,
            (false, true) => DeviceModel::Ultra,
            (true, true) => return Err(DeviceError::Conflicting),
            (false, false) => return Err(DeviceError::Unknown),
        };
        Ok(Self {
            model,
            version,
            info,
        })
    }
}

impl DeviceModel {
    /// Validate a request against this family and choose its input command.
    pub fn plan(self, request: &ScanRequest) -> Result<AcquisitionPlan, ProtocolError> {
        request.validate()?;
        let (input_command, native_zero_offset_db) = match self {
            Self::Basic => {
                if request.stop_hz > BASIC_MAX_HZ {
                    return Err(ProtocolError::InvalidRequest(
                        "tinySA Basic frequencies must be within 0–960 MHz".into(),
                    ));
                }
                let command = if request.stop_hz < 350_000_000 {
                    "mode low input"
                } else if request.start_hz >= 240_000_000 {
                    "mode high input"
                } else {
                    return Err(ProtocolError::InvalidRequest(
                        "a Basic span crossing 350 MHz must start at or above 240 MHz".into(),
                    ));
                };
                (command, BASIC_RAW_LEVEL_OFFSET_DB)
            }
            Self::Ultra => {
                if request.start_hz < ULTRA_MIN_HZ || request.stop_hz > ULTRA_NORMAL_MAX_HZ {
                    return Err(ProtocolError::InvalidRequest(
                        "tinySA Ultra first-tier support is limited to the common 0.1–800 MHz normal range; extended and harmonic modes are not enabled"
                            .into(),
                    ));
                }
                ("mode low input", ULTRA_RAW_LEVEL_OFFSET_DB)
            }
        };
        Ok(AcquisitionPlan {
            model: self,
            input_command,
            native_zero_offset_db,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(start_hz: u64, stop_hz: u64) -> ScanRequest {
        ScanRequest {
            start_hz,
            stop_hz,
            points: 101,
            repeat: 1,
        }
    }

    #[test]
    fn identity_requires_unambiguous_family_evidence() {
        assert_eq!(
            DeviceIdentity::detect("tinySA_v1.4-175-gabc".into(), "tinySA".into())
                .unwrap()
                .model,
            DeviceModel::Basic
        );
        assert_eq!(
            DeviceIdentity::detect("tinySA4_v1.4-200-gabc".into(), "tinySA Ultra+".into())
                .unwrap()
                .model,
            DeviceModel::Ultra
        );
        assert_eq!(
            DeviceIdentity::detect("version unknown".into(), "instrument".into()),
            Err(DeviceError::Unknown)
        );
        assert_eq!(
            DeviceIdentity::detect("tinySA_v1".into(), "tinySA Ultra".into()),
            Err(DeviceError::Conflicting)
        );
    }

    #[test]
    fn model_plans_enforce_paths_ranges_and_native_offsets() {
        let low = DeviceModel::Basic.plan(&request(0, 349_999_999)).unwrap();
        assert_eq!(low.input_command, "mode low input");
        assert_eq!(low.native_zero_offset_db, 128);
        let high = DeviceModel::Basic
            .plan(&request(240_000_000, 960_000_000))
            .unwrap();
        assert_eq!(high.input_command, "mode high input");
        assert!(DeviceModel::Basic.plan(&request(0, 350_000_000)).is_err());

        let ultra = DeviceModel::Ultra
            .plan(&request(ULTRA_MIN_HZ, ULTRA_NORMAL_MAX_HZ))
            .unwrap();
        assert_eq!(ultra.input_command, "mode low input");
        assert_eq!(ultra.native_zero_offset_db, 174);
        assert!(DeviceModel::Ultra.plan(&request(0, 800_000_000)).is_err());
        assert!(DeviceModel::Ultra
            .plan(&request(100_000, 800_000_001))
            .is_err());
    }
}
