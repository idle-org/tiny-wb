//! Acquisition orchestration over an injected shell connection.
//!
//! This layer owns command ordering and progress, but not port discovery, files,
//! UI state or threads. After attempting `pause`, it always attempts `resume`,
//! preserving both failures when recovery also fails. This is not full device
//! state restoration. Identity and the required command inventory are checked
//! before model-specific settings are changed.

use crate::{
    config::{ModelSelection, ScanConfig, SerialConfig},
    device::{DeviceError, DeviceIdentity, DeviceModel},
    protocol::{self, ProtocolError, Sample, ScanRequest},
    transport::{Shell, TransportError},
};
use std::{
    io::{Read, Write},
    time::Duration,
};

/// UI-neutral acquisition notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    /// Description of the active operation.
    Message(&'static str),
    /// Approximate completion percentage, from 0 through 100.
    Progress(u8),
    /// Unparsed version and information replies; not a verified model identity.
    InstrumentInfo(String),
}

/// Failure running a scan; no output file is created by this layer.
#[derive(Debug, thiserror::Error)]
pub enum AcquisitionError {
    /// An I/O or shell framing failure.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Invalid request or malformed binary frame.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// An identification reply contained no content.
    #[error("the instrument returned an empty version or info reply")]
    MissingIdentity,
    /// Identity did not unambiguously identify a supported device family.
    #[error(transparent)]
    Device(#[from] DeviceError),
    /// A configured family expectation did not match the detected device.
    #[error("configured model {expected} does not match detected {actual}")]
    UnexpectedModel {
        /// Family required by configuration.
        expected: DeviceModel,
        /// Family established from device replies.
        actual: DeviceModel,
    },
    /// The device does not advertise a command required by this scan plan.
    #[error("{model} firmware does not advertise required command {command:?}")]
    MissingCommand {
        /// Detected family.
        model: DeviceModel,
        /// Required console command verb.
        command: &'static str,
    },
    /// The firmware did not return a parseable raw-level offset after setting it.
    #[error("could not confirm the raw-level offset after configuring zero {requested}")]
    MissingZeroReadback {
        /// Offset sent to the device, in dB.
        requested: u16,
    },
    /// The raw-level offset readback differed from the requested decoder value.
    #[error("raw-level offset readback is {actual} dB after requesting {requested} dB")]
    ZeroReadbackMismatch {
        /// Offset sent to the device, in dB.
        requested: u16,
        /// Offset reported by the device, in dB.
        actual: u16,
    },
    /// Extended-mode sweep readback did not confirm the requested boundaries.
    #[error("ZS406 did not confirm the requested Ultra sweep {requested_start}–{requested_stop} Hz; readback was {response:?}")]
    SweepReadbackMismatch {
        /// Requested lower boundary, in Hz.
        requested_start: u64,
        /// Requested upper boundary, in Hz.
        requested_stop: u64,
        /// Bounded prompt-stripped firmware response.
        response: String,
    },
    /// Both acquisition and the subsequent resume attempt failed.
    #[error("{primary}; additionally, resume failed: {recovery}")]
    Recovery {
        /// Original acquisition failure.
        #[source]
        primary: Box<AcquisitionError>,
        /// Failure attempting to restart the instrument's normal sweep.
        recovery: TransportError,
    },
}

/// Detect the device, execute a supported acquisition and return decoded samples.
///
/// Settings should come from a validated [`crate::config::Config`]. The request
/// is structurally validated before I/O and checked against the detected model
/// before mutation. Basic supports its existing low/high paths through 960 MHz.
/// Ultra-family support defaults to the common normal 0.1–800 MHz range. A
/// detected ZS406 supports 900 MHz normally and opt-in Ultra mode to 5.4 GHz.
pub fn acquire<T: Read + Write>(
    shell: &mut Shell<T>,
    request: &ScanRequest,
    scan: &ScanConfig,
    serial: &SerialConfig,
    mut report: impl FnMut(ScanEvent),
) -> Result<Vec<Sample>, AcquisitionError> {
    request.validate()?;
    let delay = Duration::from_millis(serial.command_delay_ms);
    let settle = Duration::from_millis(serial.settle_delay_ms);
    report(ScanEvent::Message("Identifying instrument"));
    report(ScanEvent::Progress(5));
    shell.synchronize()?;
    let identity = identify_with_retry(shell, delay, settle, &mut report)?;
    let expected = match scan.device_model {
        ModelSelection::Auto => None,
        ModelSelection::Basic => Some(DeviceModel::Basic),
        ModelSelection::Ultra => Some(DeviceModel::Ultra),
    };
    if let Some(expected) = expected.filter(|expected| *expected != identity.model) {
        return Err(AcquisitionError::UnexpectedModel {
            expected,
            actual: identity.model,
        });
    }
    let plan = identity.plan(request, scan.enable_ultra_mode)?;
    let help = shell.command("help", delay, settle)?;
    for command in [
        "repeat", "zero", "pause", "mode", "sweep", "scanraw", "resume",
    ]
    .into_iter()
    .chain(plan.enable_ultra_mode.then_some("ultra"))
    .chain(scan.legacy_pre_scan.then_some("scan"))
    {
        if !advertises_command(&help, command) {
            return Err(AcquisitionError::MissingCommand {
                model: identity.model,
                command,
            });
        }
    }
    let revision = identity
        .ultra_revision
        .map(|revision| format!(" {revision}"))
        .unwrap_or_default();
    report(ScanEvent::InstrumentInfo(format!(
        "Detected {}{} (native raw offset {} dB)\n{}{}",
        identity.model, revision, plan.native_zero_offset_db, identity.version, identity.info
    )));
    for command in ["leveloffset", "vbat", "usart_cfg", "threads", "marker"] {
        if advertises_command(&help, command) {
            let response = shell.command(command, delay, settle)?;
            log::debug!("{command}: {response}");
        }
    }
    report(ScanEvent::Message("Configuring scan"));
    let zero_offset = if scan.auto_zero_offset {
        plan.native_zero_offset_db
    } else {
        scan.zero_offset
    };
    shell.command_checked(&format!("repeat {}", request.repeat), delay, settle)?;
    shell.command_checked(&format!("zero {zero_offset}"), delay, settle)?;
    let zero_response = shell.command("zero", delay, settle)?;
    match parse_zero_offset(&zero_response) {
        Some(actual) if actual == zero_offset => {}
        Some(actual) => {
            return Err(AcquisitionError::ZeroReadbackMismatch {
                requested: zero_offset,
                actual,
            });
        }
        None => {
            return Err(AcquisitionError::MissingZeroReadback {
                requested: zero_offset,
            });
        }
    }
    report(ScanEvent::Progress(20));
    // Pause may take effect even when its response times out, so recovery also
    // covers failure receiving the pause response itself.
    let result = (|| {
        shell.command_checked("pause", delay, settle)?;
        shell.command_checked(plan.input_command, delay, settle)?;
        if plan.enable_ultra_mode {
            shell.command_checked("ultra on", delay, settle)?;
        }
        shell.command_checked(&format!("sweep start {}", request.start_hz), delay, settle)?;
        shell.command_checked(&format!("sweep stop {}", request.stop_hz), delay, settle)?;
        let sweep_response = shell.command_checked("sweep", delay, settle)?;
        if plan.enable_ultra_mode
            && parse_sweep_bounds(&sweep_response) != Some((request.start_hz, request.stop_hz))
        {
            return Err(AcquisitionError::SweepReadbackMismatch {
                requested_start: request.start_hz,
                requested_stop: request.stop_hz,
                response: sweep_response,
            });
        }
        if scan.legacy_pre_scan {
            shell.command_checked(
                &format!("scan {} {} 290 101", request.start_hz, request.stop_hz),
                Duration::from_millis(serial.scan_delay_ms),
                settle,
            )?;
        }
        report(ScanEvent::Message("Acquiring samples"));
        shell.send(&format!(
            "scanraw {} {} {}",
            request.start_hz, request.stop_hz, request.points
        ))?;
        let frame = shell.read_frame(request, |received, expected| {
            report(ScanEvent::Progress((20 + received * 75 / expected) as u8))
        })?;
        Ok(protocol::decode_frame(
            &frame,
            request,
            zero_offset,
            scan.frequency_axis,
        )?)
    })();
    let recovery = shell.command_checked("resume", delay, settle);
    match (result, recovery) {
        (Ok(samples), Ok(_)) => {
            report(ScanEvent::Message("Acquisition complete"));
            report(ScanEvent::Progress(100));
            Ok(samples)
        }
        (Err(primary), Err(recovery)) => Err(AcquisitionError::Recovery {
            primary: Box::new(primary),
            recovery,
        }),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

/// Retry the read-only identity exchange once after a complete but unusable reply.
///
/// Transport failures are not retried because a timeout can leave an incomplete
/// prompt in the buffered stream. Replaying commands in that state would risk
/// assigning a late response to the wrong request.
fn identify_with_retry<T: Read + Write>(
    shell: &mut Shell<T>,
    delay: Duration,
    settle: Duration,
    report: &mut impl FnMut(ScanEvent),
) -> Result<DeviceIdentity, AcquisitionError> {
    for attempt in 0..2 {
        let version = shell.command("version", delay, settle)?;
        let info = shell.command("info", delay, settle)?;
        let identity = if version.trim().is_empty() || info.trim().is_empty() {
            Err(AcquisitionError::MissingIdentity)
        } else {
            DeviceIdentity::detect(version, info).map_err(AcquisitionError::from)
        };
        match identity {
            Ok(identity) => return Ok(identity),
            Err(error) if attempt == 0 => {
                log::warn!("instrument identification failed; retrying once: {error}");
                report(ScanEvent::Message("Retrying instrument identification"));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the bounded identity loop always returns")
}

fn advertises_command(help: &str, command: &str) -> bool {
    help.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|word| word.eq_ignore_ascii_case(command))
}

fn parse_zero_offset(response: &str) -> Option<u16> {
    response.split_whitespace().find_map(|word| {
        word.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '-')
            .to_ascii_lowercase()
            .strip_suffix("dbm")
            .and_then(|value| value.parse().ok())
    })
}

fn parse_sweep_bounds(response: &str) -> Option<(u64, u64)> {
    response.lines().find_map(|line| {
        let values: Vec<_> = line
            .split_whitespace()
            .filter_map(|word| word.parse::<u64>().ok())
            .collect();
        (values.len() >= 2).then(|| (values[0], values[1]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::{
        io::{self, Cursor},
        sync::{Arc, Mutex},
    };

    /// Scripted replies plus shared command capture, independent of serialport.
    struct Transcript {
        input: Cursor<Vec<u8>>,
        commands: Arc<Mutex<Vec<u8>>>,
    }
    impl Read for Transcript {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.input.read(bytes)
        }
    }
    impl Write for Transcript {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.commands.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn exercise(
        frame: &[u8],
        resume: bool,
        legacy: bool,
    ) -> (
        Result<Vec<Sample>, AcquisitionError>,
        String,
        Vec<ScanEvent>,
    ) {
        let mut config = Config::default();
        config.serial.command_delay_ms = 0;
        config.serial.settle_delay_ms = 0;
        config.serial.scan_delay_ms = 0;
        config.scan.legacy_pre_scan = legacy;
        let request = ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 960_000_000,
            points: 2,
            repeat: 1,
        };
        let mut input = b"ch>tinySA_v1.4-test\r\nch>tinySA\r\nch>Commands: repeat zero pause mode sweep scan scanraw resume leveloffset vbat usart_cfg threads marker\r\nch>".to_vec();
        // Five optional diagnostics, repeat, zero mutation/readback, pause, mode and sweep commands.
        input.extend_from_slice(b"okch>".repeat(7).as_slice());
        input.extend_from_slice(b"usage: zero {level}\r\n128dBm\r\nch>");
        input.extend_from_slice(b"okch>".repeat(5 + usize::from(legacy)).as_slice());
        input.extend_from_slice(frame);
        input.extend_from_slice(b"ch>");
        if resume {
            input.extend_from_slice(b"okch>");
        }
        let commands = Arc::new(Mutex::new(Vec::new()));
        let mut shell = Shell::new(
            Transcript {
                input: Cursor::new(input),
                commands: commands.clone(),
            },
            256,
        );
        let mut events = Vec::new();
        let result = acquire(
            &mut shell,
            &request,
            &config.scan,
            &config.serial,
            |event| events.push(event),
        );
        let written = String::from_utf8(commands.lock().unwrap().clone()).unwrap();
        (result, written, events)
    }

    #[test]
    fn full_acquisition_preserves_command_order_and_reports_progress() {
        let (result, commands, events) = exercise(b"{x\0\x08x\0\x10}", true, true);
        assert_eq!(result.unwrap().len(), 2);
        assert_eq!(commands, "version\rinfo\rhelp\rleveloffset\rvbat\rusart_cfg\rthreads\rmarker\rrepeat 1\rzero 128\rzero\rpause\rmode high input\rsweep start 240000000\rsweep stop 960000000\rsweep\rscan 240000000 960000000 290 101\rscanraw 240000000 960000000 2\rresume\r");
        assert_eq!(events.last(), Some(&ScanEvent::Progress(100)));
        assert!(events.iter().any(|event| matches!(event, ScanEvent::InstrumentInfo(info) if info.contains("Detected tinySA Basic"))));
    }

    #[test]
    fn legacy_pre_scan_can_be_disabled() {
        let (result, commands, _) = exercise(b"{x\0\x08x\0\x10}", true, false);
        assert!(result.is_ok());
        assert!(!commands.contains("\rscan "));
    }

    #[test]
    fn identification_retries_once_after_an_unusable_version() {
        let mut config = Config::default();
        config.scan.legacy_pre_scan = false;
        config.serial.command_delay_ms = 0;
        config.serial.settle_delay_ms = 0;
        let request = ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 960_000_000,
            points: 2,
            repeat: 1,
        };
        let mut input = b"ch>\r\nch>unknown\r\nch>tinySA_v1.4-test\r\nch>tinySA\r\nch>Commands: repeat zero pause mode sweep scanraw resume\r\nch>"
            .to_vec();
        input.extend_from_slice(b"okch>".repeat(2).as_slice());
        input.extend_from_slice(b"128dBm\r\nch>");
        input.extend_from_slice(b"okch>".repeat(5).as_slice());
        input.extend_from_slice(b"{x\0\x08x\0\x10}ch>okch>");
        let commands = Arc::new(Mutex::new(Vec::new()));
        let mut shell = Shell::new(
            Transcript {
                input: Cursor::new(input),
                commands: commands.clone(),
            },
            1024,
        );
        let mut events = Vec::new();

        let result = acquire(
            &mut shell,
            &request,
            &config.scan,
            &config.serial,
            |event| events.push(event),
        );

        assert!(result.is_ok(), "{result:?}");
        let written = String::from_utf8(commands.lock().unwrap().clone()).unwrap();
        assert!(written.starts_with("version\rinfo\rversion\rinfo\rhelp\r"));
        assert_eq!(written.matches("version\r").count(), 2);
        assert!(events.contains(&ScanEvent::Message("Retrying instrument identification")));
    }

    #[test]
    fn malformed_frames_still_resume_and_preserve_original_error() {
        let (result, commands, _) = exercise(b"{y\0\x08x\0\x10}", true, true);
        assert!(matches!(
            result,
            Err(AcquisitionError::Protocol(ProtocolError::Alignment {
                index: 0,
                ..
            }))
        ));
        assert!(commands.ends_with("resume\r"));
        let (result, commands, _) = exercise(b"{y\0\x08x\0\x10}", false, true);
        assert!(matches!(result, Err(AcquisitionError::Recovery { .. })));
        assert!(commands.ends_with("resume\r"));
    }

    #[test]
    fn help_and_zero_readback_parsers_are_token_bounded() {
        assert!(advertises_command("Commands: scan scanraw zero", "scan"));
        assert!(!advertises_command("Commands: scanraw", "scan"));
        assert_eq!(
            parse_zero_offset("usage: zero {level}\r\n174dBm\r\n"),
            Some(174)
        );
        assert_eq!(parse_zero_offset("174 dBm"), None);
        assert_eq!(
            parse_sweep_bounds("sweep\r\n900000000 5400000000 450\r\n"),
            Some((900_000_000, 5_400_000_000))
        );
        assert_eq!(parse_sweep_bounds("ok"), None);
    }

    #[test]
    fn failed_detection_capabilities_and_readback_stop_before_scan() {
        for (version, info, help, zero, expected) in [
            ("unknown", "unknown", "", "", "unsupported device"),
            ("tinySA_v1", "tinySA Ultra", "", "", "conflicting"),
            (
                "tinySA4_v1",
                "tinySA Ultra",
                "repeat",
                "",
                "required command",
            ),
            (
                "tinySA4_v1",
                "tinySA Ultra",
                "repeat zero pause mode sweep scanraw resume",
                "128dBm",
                "readback is 128",
            ),
            (
                "tinySA4_v1",
                "tinySA Ultra",
                "repeat zero pause mode sweep scanraw resume",
                "unknown",
                "could not confirm",
            ),
        ] {
            let config = Config::from_toml(
                "[scan]\nlegacy_pre_scan=false\n[serial]\ncommand_delay_ms=0\nsettle_delay_ms=0",
            )
            .unwrap();
            let commands = Arc::new(Mutex::new(Vec::new()));
            let retry_identity = if matches!(expected, "unsupported device" | "conflicting") {
                format!("{version}\r\nch>{info}\r\nch>")
            } else {
                String::new()
            };
            let input = format!(
                "ch>{version}\r\nch>{info}\r\nch>{retry_identity}{help}\r\nch>ch>ch>{zero}\r\nch>"
            );
            let mut shell = Shell::new(
                Transcript {
                    input: Cursor::new(input.into_bytes()),
                    commands: commands.clone(),
                },
                1024,
            );
            let request = ScanRequest {
                start_hz: 100_000_000,
                stop_hz: 800_000_000,
                points: 2,
                repeat: 1,
            };
            let error =
                acquire(&mut shell, &request, &config.scan, &config.serial, |_| {}).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            let written = String::from_utf8(commands.lock().unwrap().clone()).unwrap();
            assert!(!written.contains("pause\r"));
            assert!(!written.contains("scanraw "));
        }
    }

    #[test]
    fn zs406_extended_scan_is_opt_in_and_verifies_sweep_readback() {
        let request = ScanRequest {
            start_hz: 900_000_000,
            stop_hz: 5_400_000_000,
            points: 2,
            repeat: 1,
        };
        let run = |enable_ultra_mode: bool, sweep: &str| {
            let mut config = Config::default();
            config.scan.enable_ultra_mode = enable_ultra_mode;
            config.scan.legacy_pre_scan = false;
            config.serial.command_delay_ms = 0;
            config.serial.settle_delay_ms = 0;
            let input = format!(
                "ch>tinySA4_v1.4-test\r\nHW Version:V0.4.6\r\nch>tinySA Ultra+ ZS406\r\nch>Commands: repeat zero pause mode ultra sweep scanraw resume\r\nch>ch>ch>174dBm\r\nch>ch>ch>ch>ch>ch>{sweep}\r\nch>{{x\0\x08x\0\x10}}ch>ch>"
            );
            let commands = Arc::new(Mutex::new(Vec::new()));
            let mut shell = Shell::new(
                Transcript {
                    input: Cursor::new(input.into_bytes()),
                    commands: commands.clone(),
                },
                1024,
            );
            let result = acquire(&mut shell, &request, &config.scan, &config.serial, |_| {});
            let written = String::from_utf8(commands.lock().unwrap().clone()).unwrap();
            (result, written)
        };

        let (result, commands) = run(false, "900000000 5400000000 450");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("enable_ultra_mode"));
        assert!(!commands.contains("pause\r"));

        let (result, commands) = run(true, "900000000 5400000000 450");
        assert!(result.is_ok(), "{result:?}");
        assert!(commands.contains("mode low input\rultra on\r"));
        assert!(commands.contains("scanraw 900000000 5400000000 2\r"));

        let (result, commands) = run(true, "900000000 900000000 450");
        assert!(matches!(
            result,
            Err(AcquisitionError::SweepReadbackMismatch { .. })
        ));
        assert!(!commands.contains("scanraw "));
        assert!(commands.ends_with("resume\r"));
    }
}
