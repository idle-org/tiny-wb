//! Typed configuration with embedded defaults and validated partial overrides.
//!
//! The six original top-level selection keys remain supported. Missing files
//! use defaults; unreadable files, unknown keys and invalid values are errors.
//! Paths in settings are relative to the process working directory.

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Human-readable defaults, also used by tests and packaged with the binary.
pub const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");

/// Failures reading, validating or persisting configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The path could not be read or atomically replaced.
    #[error("configuration I/O: {0}")]
    Io(#[from] io::Error),
    /// TOML syntax or a typed setting is invalid.
    #[error("invalid configuration: {0}")]
    Parse(#[from] toml::de::Error),
    /// Serialization failed before replacing the existing file.
    #[error("cannot serialize configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// A setting violates an application invariant.
    #[error("invalid configuration: {0}")]
    Validation(String),
}

/// Complete application settings; fields retain units from the configuration file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Preferred serial port; empty selects the first available candidate.
    pub port: String,
    /// Selected baud rate, which must occur in `serial.baudrates`.
    pub baudrate: u32,
    /// Scan start in MHz, within the Basic 0–960 MHz envelope.
    pub start_frequency: usize,
    /// Scan stop in MHz, no lower than the start.
    pub stop_frequency: usize,
    /// Number of samples in the raw scan.
    pub points: usize,
    /// Measurements per point.
    pub repeat: usize,
    /// Serial or TCP transport selection; TCP is useful for bridges and simulation.
    pub connection: ConnectionConfig,
    /// Serial timing, response limits and selection preferences.
    pub serial: SerialConfig,
    /// Acquisition and numeric input policy.
    pub scan: ScanConfig,
    /// Rendering and event timing.
    pub ui: UiConfig,
    /// CSV destination and format.
    pub output: OutputConfig,
    /// File logging settings.
    pub logging: LoggingConfig,
}

/// Transport selected by the application, without changing shell framing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// A native USB/serial port (requires the `serial` Cargo feature).
    Serial,
    /// A TCP bridge or mock server; tinySA hardware does not provide native TCP.
    Tcp,
}

/// Connection settings; read deadlines apply to individual I/O operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionConfig {
    /// Which connection adapter to open.
    pub backend: Backend,
    /// Numeric IPv4/IPv6 socket address used for TCP (no DNS lookup).
    pub tcp_address: String,
    /// TCP connect, read and write timeout, in milliseconds.
    pub tcp_timeout_ms: u64,
}

/// Serial settings, separate from the protocol's fixed byte grammar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SerialConfig {
    /// Maximum idle wait for each read, in milliseconds.
    pub timeout_ms: u64,
    /// Delay after sending ordinary commands, in milliseconds.
    pub command_delay_ms: u64,
    /// Delay after a response, in milliseconds.
    pub settle_delay_ms: u64,
    /// Delay after the legacy preparatory scan, in milliseconds.
    pub scan_delay_ms: u64,
    /// Maximum text response or pre-frame echo size, in bytes.
    pub max_response_bytes: usize,
    /// Selectable nonzero baud rates, without duplicates.
    pub baudrates: Vec<u32>,
    /// Fallback port names, tried in order after `port`.
    pub preferred_ports: Vec<String>,
}

/// Frequency labeling policy; this does not change the instrument's sweep.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FrequencyAxis {
    /// Preserve legacy tiny-wb labels, including the requested stop frequency.
    Inclusive,
    /// Match the reviewed `scanraw` integer and single-precision calculation.
    Firmware,
}

/// Optional identity expectation; acquisition always detects the connected family.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSelection {
    /// Accept either recognized family and apply its profile automatically.
    Auto,
    /// Require a detected tinySA Basic.
    Basic,
    /// Require a detected tinySA Ultra/Ultra+ family device.
    Ultra,
}

/// Scan settings; model-specific frequency limits are enforced by device plans.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScanConfig {
    /// Expected device family; `auto` still rejects unknown or conflicting identity.
    pub device_model: ModelSelection,
    /// Select the detected model's native raw-level offset.
    pub auto_zero_offset: bool,
    /// Offset sent with `zero` and subtracted from raw levels, in dB.
    /// Used only when `auto_zero_offset` is false.
    pub zero_offset: u16,
    /// Keyboard step for frequencies, in MHz.
    pub frequency_step_mhz: usize,
    /// Keyboard step for sample count.
    pub points_step: usize,
    /// Lowest selectable sample count; at least two.
    pub min_points: usize,
    /// Highest selectable sample count; at most 1,000,001.
    pub max_points: usize,
    /// Highest repeat count; at most the firmware limit of 1,000.
    pub max_repeat: usize,
    /// Host-side frequency labeling convention.
    pub frequency_axis: FrequencyAxis,
    /// Whether to retain the legacy `scan START STOP 290 101` preparation.
    pub legacy_pre_scan: bool,
}

/// Settings for the terminal presentation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiConfig {
    /// Maximum event polling interval, in milliseconds.
    pub poll_interval_ms: u64,
    /// Chart lower bound, in dBm; finite and lower than the upper bound.
    pub chart_min_dbm: f64,
    /// Chart upper bound, in dBm.
    pub chart_max_dbm: f64,
}

/// CSV settings; paths are relative to the working directory unless absolute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Directory for completed scans; created on export if absent.
    pub directory: PathBuf,
    /// Filename prefix containing only ASCII letters, digits, `_` or `-`.
    pub filename_prefix: String,
    /// Emit a `frequency_MHz,level_dBm` header when true.
    pub include_header: bool,
}

/// Settings accepted by the file logger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    /// flexi_logger filter expression, for example `info,tiny_wb=debug`.
    pub filter: String,
    /// Log destination directory.
    pub directory: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG).expect("embedded configuration must be valid")
    }
}

impl Config {
    /// Overlay a partial TOML document onto embedded defaults and validate it.
    ///
    /// # Errors
    /// Returns an error for unknown keys, malformed TOML or invalid settings.
    pub fn from_toml(contents: &str) -> Result<Self, ConfigError> {
        let mut defaults: toml::Value = toml::from_str(DEFAULT_CONFIG)?;
        merge(&mut defaults, toml::from_str(contents)?);
        let config: Self = defaults.try_into()?;
        config.validate()?;
        Ok(config)
    }

    /// Load settings, using defaults only if the path does not exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(contents) => Self::from_toml(&contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Validate and atomically replace settings without depending on UI state.
    ///
    /// All recognized sections survive saving; formatting and comments do not.
    /// The parent directory must exist. The old file survives a failed write.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        crate::storage::replace(path, toml::to_string_pretty(self)?.as_bytes())?;
        Ok(())
    }

    /// Check bounds before values become allocations, durations or scan commands.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let require = |condition, message: &str| {
            if condition {
                Ok(())
            } else {
                Err(ConfigError::Validation(message.into()))
            }
        };
        require(
            !self.serial.baudrates.is_empty() && self.serial.baudrates.iter().all(|&v| v > 0),
            "serial.baudrates must contain nonzero values",
        )?;
        let mut rates = self.serial.baudrates.clone();
        rates.sort_unstable();
        rates.dedup();
        require(
            rates.len() == self.serial.baudrates.len(),
            "serial.baudrates must not contain duplicates",
        )?;
        require(
            rates.contains(&self.baudrate),
            "baudrate must be in serial.baudrates",
        )?;
        require(
            self.start_frequency <= self.stop_frequency
                && self.stop_frequency <= 960
                && self.stop_frequency >= 10,
            "frequencies must be ordered in 0–960 MHz, with stop at least 10 MHz",
        )?;
        require(
            self.scan.min_points >= 2
                && self.scan.min_points <= self.scan.max_points
                && self.scan.max_points <= 1_000_001,
            "scan point bounds must satisfy 2 <= min_points <= max_points <= 1000001",
        )?;
        require(
            (self.scan.min_points..=self.scan.max_points).contains(&self.points),
            "points must be within scan.min_points and scan.max_points",
        )?;
        require(
            (1..=1000).contains(&self.scan.max_repeat)
                && (1..=self.scan.max_repeat).contains(&self.repeat),
            "repeat must be within 1..=scan.max_repeat, which cannot exceed 1000",
        )?;
        require(
            (1..=960).contains(&self.scan.frequency_step_mhz)
                && (1..=self.scan.max_points).contains(&self.scan.points_step),
            "scan keyboard steps must be positive and within their bounds",
        )?;
        require(
            self.scan.zero_offset <= 2047,
            "scan.zero_offset must fit the 16-bit fixed-point encoding (0..=2047)",
        )?;
        require(
            (1..=600_000).contains(&self.serial.timeout_ms),
            "serial.timeout_ms must be in 1..=600000",
        )?;
        require(
            (1..=600_000).contains(&self.connection.tcp_timeout_ms),
            "connection.tcp_timeout_ms must be in 1..=600000",
        )?;
        require(
            self.connection
                .tcp_address
                .parse::<std::net::SocketAddr>()
                .is_ok_and(|address| address.port() != 0),
            "connection.tcp_address must be a numeric IP:port with a nonzero port",
        )?;
        for delay in [
            self.serial.command_delay_ms,
            self.serial.settle_delay_ms,
            self.serial.scan_delay_ms,
        ] {
            require(
                delay <= 60_000,
                "serial command delays cannot exceed 60000 ms",
            )?;
        }
        require(
            (256..=16_777_216).contains(&self.serial.max_response_bytes),
            "serial.max_response_bytes must be in 256..=16777216",
        )?;
        require(
            (1..=1000).contains(&self.ui.poll_interval_ms),
            "ui.poll_interval_ms must be in 1..=1000",
        )?;
        require(
            self.ui.chart_min_dbm.is_finite()
                && self.ui.chart_max_dbm.is_finite()
                && self.ui.chart_min_dbm < self.ui.chart_max_dbm,
            "UI chart bounds must be finite and increasing",
        )?;
        require(!self.output.filename_prefix.is_empty() && self.output.filename_prefix.len() <= 64 && self.output.filename_prefix.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'), "output.filename_prefix must contain 1–64 ASCII letters, digits, underscores or hyphens")?;
        require(
            !self.output.directory.as_os_str().is_empty()
                && !self.logging.directory.as_os_str().is_empty(),
            "output and logging directories must not be empty",
        )?;
        require(
            !self.logging.filter.trim().is_empty(),
            "logging.filter must not be empty",
        )
    }
}

/// Recursively overlay supplied tables; scalar and array values replace defaults.
fn merge(base: &mut toml::Value, overrides: toml::Value) {
    match (base, overrides) {
        (toml::Value::Table(base), toml::Value::Table(overrides)) => {
            for (key, value) in overrides {
                if let Some(original) = base.get_mut(&key) {
                    merge(original, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, value) => *base = value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_legacy_settings_are_supported() {
        Config::default().validate().unwrap();
        assert_eq!(
            Config::default().scan.frequency_axis,
            FrequencyAxis::Firmware
        );
        let config = Config::from_toml("port = 'COM3'\nbaudrate = 57600\nstart_frequency = 20\nstop_frequency = 100\npoints = 101\nrepeat = 2").unwrap();
        assert_eq!(config.port, "COM3");
        assert_eq!(config.points, 101);
        assert_eq!(config.serial, Config::default().serial);
        assert_eq!(
            Config::from_toml("[scan]\nfrequency_axis = 'inclusive'")
                .unwrap()
                .scan
                .frequency_axis,
            FrequencyAxis::Inclusive
        );
    }

    #[test]
    fn partial_sections_merge_and_round_trip() {
        let config =
            Config::from_toml("[serial]\ntimeout_ms = 5000\n[output]\ninclude_header = true")
                .unwrap();
        assert_eq!(config.serial.timeout_ms, 5000);
        assert_eq!(config.serial.baudrates, Config::default().serial.baudrates);
        assert!(config.output.include_header);
        assert_eq!(
            Config::from_toml(&toml::to_string(&config).unwrap()).unwrap(),
            config
        );
    }

    #[test]
    fn malformed_unknown_and_unsafe_settings_are_rejected() {
        for input in [
            "broken",
            "pointz = 100",
            "points = 1",
            "start_frequency = 961",
            "start_frequency = 900\nstop_frequency = 100",
            "repeat = 0",
            "[serial]\nbaudrates = []",
            "[serial]\nbaudrates = [115200, 115200]",
            "[serial]\ntimeout_ms = 0",
            "[serial]\nmax_response_bytes = 0",
            "[scan]\nmin_points = 0",
            "[scan]\nfrequency_step_mhz = 0",
            "[scan]\nzero_offset = 2048",
            "[scan]\ndevice_model = 'unknown'",
            "[scan]\nfrequency_axis = 'unknown'",
            "[ui]\nchart_min_dbm = nan",
            "[ui]\nchart_min_dbm = 1.0",
            "[output]\nfilename_prefix = '../scan'",
            "[logging]\nfilter = ''",
            "[serial]\ntimeuot_ms = 5",
        ] {
            assert!(Config::from_toml(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn persistence_preserves_settings_and_rejects_invalid_replacement() {
        let directory = std::env::temp_dir().join(format!("tiny-wb-config-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.toml");
        assert_eq!(
            Config::load(&directory.join("missing")).unwrap(),
            Config::default()
        );
        let mut config = Config::from_toml("[output]\ninclude_header = true").unwrap();
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        let original = fs::read(&path).unwrap();
        config.points = 0;
        assert!(config.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(Config::load(&directory).is_err());
        fs::write(&path, "unknown = true").unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
