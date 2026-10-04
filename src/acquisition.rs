//! Acquisition orchestration over an injected shell connection.
//!
//! This layer owns command ordering and progress, but not port discovery, files,
//! UI state or threads. After attempting `pause`, it always attempts `resume`,
//! preserving both failures when recovery also fails. This is not full device
//! state restoration or capability negotiation.

use crate::{
    config::{ScanConfig, SerialConfig},
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

/// Execute a Basic acquisition and return validated samples without file I/O.
///
/// Settings should come from a validated [`crate::config::Config`]. The request
/// is revalidated before any command. Version/info are retained verbatim; their
/// presence does not prove the device accepted all later commands.
pub fn acquire<T: Read + Write>(
    shell: &mut Shell<T>,
    request: &ScanRequest,
    scan: &ScanConfig,
    serial: &SerialConfig,
    mut report: impl FnMut(ScanEvent),
) -> Result<Vec<Sample>, AcquisitionError> {
    let mode = request.input_mode()?;
    let delay = Duration::from_millis(serial.command_delay_ms);
    let settle = Duration::from_millis(serial.settle_delay_ms);
    report(ScanEvent::Message("Identifying instrument"));
    report(ScanEvent::Progress(5));
    shell.synchronize()?;
    let version = shell.command("version", delay, settle)?;
    let info = shell.command("info", delay, settle)?;
    if version.trim().is_empty() || info.trim().is_empty() {
        return Err(AcquisitionError::MissingIdentity);
    }
    report(ScanEvent::InstrumentInfo(format!("{version}{info}")));
    for command in ["leveloffset", "vbat", "usart_cfg", "threads"] {
        let response = shell.command(command, delay, settle)?;
        log::debug!("{command}: {response}");
    }
    report(ScanEvent::Message("Configuring scan"));
    shell.command(&format!("repeat {}", request.repeat), delay, settle)?;
    shell.command(&format!("zero {}", scan.zero_offset), delay, settle)?;
    shell.command("marker", delay, settle)?;
    report(ScanEvent::Progress(20));
    // Pause may take effect even when its response times out, so recovery also
    // covers failure receiving the pause response itself.
    let result = (|| {
        shell.command("pause", delay, settle)?;
        shell.command(&format!("mode {} input", mode.as_str()), delay, settle)?;
        shell.command(&format!("sweep start {}", request.start_hz), delay, settle)?;
        shell.command(&format!("sweep stop {}", request.stop_hz), delay, settle)?;
        shell.command("sweep", delay, settle)?;
        if scan.legacy_pre_scan {
            shell.command(
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
            scan.zero_offset,
            scan.frequency_axis,
        )?)
    })();
    let recovery = shell.command("resume", delay, settle);
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
        let mut input = b"ch>version 1\rch>Basic\rch>".to_vec();
        // Four diagnostic queries, repeat/zero/marker, pause, mode and three sweep commands.
        input.extend_from_slice(b"okch>".repeat(12 + usize::from(legacy)).as_slice());
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
        assert_eq!(commands, "version\rinfo\rleveloffset\rvbat\rusart_cfg\rthreads\rrepeat 1\rzero 128\rmarker\rpause\rmode high input\rsweep start 240000000\rsweep stop 960000000\rsweep\rscan 240000000 960000000 290 101\rscanraw 240000000 960000000 2\rresume\r");
        assert_eq!(events.last(), Some(&ScanEvent::Progress(100)));
    }

    #[test]
    fn legacy_pre_scan_can_be_disabled() {
        let (result, commands, _) = exercise(b"{x\0\x08x\0\x10}", true, false);
        assert!(result.is_ok());
        assert!(!commands.contains("\rscan "));
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
}
