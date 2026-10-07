//! Device identity parsing and model-specific acquisition policy.
//!
//! Identification consumes explicit `version`/`info` evidence. This module
//! selects acquisition behavior only from explicit family and hardware-revision
//! evidence. ZS406 Ultra mode is opt-in; harmonic mode remains unsupported.

use crate::protocol::{
    ProtocolError, ScanRequest, BASIC_MAX_HZ, BASIC_RAW_LEVEL_OFFSET_DB, ULTRA_RAW_LEVEL_OFFSET_DB,
};
use std::fmt;

/// Lowest documented Ultra analyzer frequency, in Hz.
pub const ULTRA_MIN_HZ: u64 = 100_000;
/// Common normal-mode ceiling across supported Ultra family revisions, in Hz.
pub const ULTRA_NORMAL_MAX_HZ: u64 = 800_000_000;
/// ZS406 normal-mode ceiling, in Hz.
pub const ZS406_NORMAL_MAX_HZ: u64 = 900_000_000;
/// Highest ZS406 frequency supported without harmonic mixing, in Hz.
pub const ZS406_ULTRA_MAX_HZ: u64 = 5_400_000_000;

/// Device family established from console identity replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceModel {
    /// Original 2.8-inch tinySA with low and high input paths.
    Basic,
    /// tinySA Ultra/Ultra+ family, restricted here to the common normal range.
    Ultra,
}

/// Hardware revision established from the firmware's `version`/`info` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UltraRevision {
    /// Original tinySA Ultra hardware.
    Zs405,
    /// First tinySA Ultra+ hardware, version V0.4.6.
    Zs406,
    /// Later tinySA Ultra+ hardware; retained on the conservative family range.
    Zs407,
}

impl fmt::Display for UltraRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Zs405 => "ZS405",
            Self::Zs406 => "ZS406",
            Self::Zs407 => "ZS407",
        })
    }
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
    /// Ultra-family hardware revision, when the replies contain explicit evidence.
    pub ultra_revision: Option<UltraRevision>,
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
    /// Whether the acquisition must explicitly enable firmware Ultra mode.
    pub enable_ultra_mode: bool,
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
    /// More than one Ultra revision signature occurred in the identity exchange.
    #[error("conflicting device identity: replies identify multiple tinySA Ultra revisions")]
    ConflictingRevision,
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
        let identity_lower = format!("{version_lower}\n{info_lower}");
        let revision_matches = [
            (
                UltraRevision::Zs405,
                identity_lower.contains("zs405") || identity_lower.contains("hw version:v0.4.5.1"),
            ),
            (
                UltraRevision::Zs406,
                identity_lower.contains("zs406") || identity_lower.contains("hw version:v0.4.6"),
            ),
            (
                UltraRevision::Zs407,
                identity_lower.contains("zs407")
                    || identity_lower.contains("hw version:v0.5.3")
                    || identity_lower.contains("hw version:v0.5.4"),
            ),
        ];
        let mut revisions = revision_matches
            .into_iter()
            .filter_map(|(revision, matches)| matches.then_some(revision));
        let ultra_revision = revisions.next();
        if revisions.next().is_some() {
            return Err(DeviceError::ConflictingRevision);
        }
        let ultra_revision = (model == DeviceModel::Ultra)
            .then_some(ultra_revision)
            .flatten();
        Ok(Self {
            model,
            ultra_revision,
            version,
            info,
        })
    }

    /// Validate a request using detected revision evidence and explicit Ultra opt-in.
    pub fn plan(
        &self,
        request: &ScanRequest,
        allow_ultra_mode: bool,
    ) -> Result<AcquisitionPlan, ProtocolError> {
        if self.model == DeviceModel::Basic {
            return self.model.plan(request);
        }
        request.validate()?;
        if request.start_hz < ULTRA_MIN_HZ {
            return Err(ProtocolError::InvalidRequest(
                "tinySA Ultra frequencies must start at or above 0.1 MHz".into(),
            ));
        }
        let normal_max_hz = match self.ultra_revision {
            Some(UltraRevision::Zs406) => ZS406_NORMAL_MAX_HZ,
            Some(UltraRevision::Zs405 | UltraRevision::Zs407) | None => ULTRA_NORMAL_MAX_HZ,
        };
        let enable_ultra_mode = request.stop_hz > normal_max_hz;
        if enable_ultra_mode {
            if self.ultra_revision != Some(UltraRevision::Zs406) {
                return Err(ProtocolError::InvalidRequest(
                    "extended acquisition currently requires an explicitly detected ZS406".into(),
                ));
            }
            if !allow_ultra_mode {
                return Err(ProtocolError::InvalidRequest(
                    "ZS406 scans above 900 MHz require scan.enable_ultra_mode = true".into(),
                ));
            }
            if request.stop_hz > ZS406_ULTRA_MAX_HZ {
                return Err(ProtocolError::InvalidRequest(
                    "ZS406 support stops at 5.4 GHz; harmonic mode is not supported".into(),
                ));
            }
        }
        Ok(AcquisitionPlan {
            model: self.model,
            input_command: "mode low input",
            native_zero_offset_db: ULTRA_RAW_LEVEL_OFFSET_DB,
            enable_ultra_mode,
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
            enable_ultra_mode: false,
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
            DeviceIdentity::detect(
                "tinySA4_v1.4-200-gabc\r\nHW Version:V0.4.6".into(),
                "tinySA Ultra+ ZS406".into()
            )
            .unwrap()
            .model,
            DeviceModel::Ultra
        );
        assert_eq!(
            DeviceIdentity::detect(
                "tinySA4_v1.4-200-gabc\r\nHW Version:V0.4.6".into(),
                "tinySA Ultra+ ZS406".into()
            )
            .unwrap()
            .ultra_revision,
            Some(UltraRevision::Zs406)
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

    #[test]
    fn zs406_plan_separates_normal_ultra_and_harmonic_ranges() {
        let identity = DeviceIdentity::detect(
            "tinySA4_v1.4-test\r\nHW Version:V0.4.6".into(),
            "tinySA Ultra+ ZS406".into(),
        )
        .unwrap();
        let normal = identity
            .plan(&request(ULTRA_MIN_HZ, ZS406_NORMAL_MAX_HZ), false)
            .unwrap();
        assert!(!normal.enable_ultra_mode);
        assert!(identity
            .plan(&request(900_000_000, ZS406_ULTRA_MAX_HZ), false)
            .is_err());
        let ultra = identity
            .plan(&request(900_000_000, ZS406_ULTRA_MAX_HZ), true)
            .unwrap();
        assert!(ultra.enable_ultra_mode);
        assert!(identity
            .plan(&request(900_000_000, ZS406_ULTRA_MAX_HZ + 1), true)
            .is_err());

        let zs407 = DeviceIdentity::detect(
            "tinySA4_v1.4-test\r\nHW Version:V0.5.4".into(),
            "tinySA Ultra+ ZS407".into(),
        )
        .unwrap();
        assert!(zs407
            .plan(&request(ULTRA_MIN_HZ, ULTRA_NORMAL_MAX_HZ), false)
            .is_ok());
        assert!(zs407
            .plan(&request(ULTRA_MIN_HZ, ULTRA_NORMAL_MAX_HZ + 1), true)
            .is_err());
    }
}
