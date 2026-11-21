//! Serial worker adapter and shared progress state for the terminal application.
//!
//! The worker opens the device, retries an unusable identity on one fresh
//! connection, and exports completed samples. Protocol parsing
//! and orchestration stay in the library; no core type knows about UI widgets.

use crate::error::{AppError, UiError};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tiny_wb::{
    acquisition::{acquire, acquire_live, AcquisitionError, ScanEvent},
    config::Config,
    connection::{open_connection, ConnectionError},
    device::DeviceError,
    export::save_csv,
    live::{TraceBuffer, TraceMode},
    protocol::Sample,
    protocol::ScanRequest,
    transport::Shell,
};

/// Acquisition lifecycle visible to the UI.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanStatus {
    /// No worker has started.
    #[default]
    Ready,
    /// Port opening, acquisition or CSV export is in progress.
    Running,
    /// Samples and CSV are available.
    Complete,
    /// A worker or UI transition failed.
    Failed,
}

/// Data protected by one lock so terminal state changes are coherent.
#[derive(Debug, Default)]
struct Progress {
    /// Completion estimate, clamped to 0–100.
    percent: usize,
    /// Lifecycle status.
    status: ScanStatus,
    /// Current operation or completion message.
    message: String,
    /// Verbatim identification replies.
    info: String,
    /// Samples converted to the chart's `(MHz, dBm)` representation.
    samples: Option<Vec<(f64, f64)>>,
    /// Completed output path.
    filename: Option<PathBuf>,
    /// User-readable error with its underlying context.
    error: Option<String>,
}

/// Cloneable progress handle; acquisition parameters are passed separately.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScanState(Arc<Mutex<Progress>>);

/// Shared repeated-snapshot state for the live terminal screen.
#[derive(Debug, Clone)]
pub(crate) struct LiveState(Arc<Mutex<LiveProgress>>);

/// One coherent live snapshot and its control flags.
#[derive(Debug)]
struct LiveProgress {
    /// Worker lifecycle.
    status: ScanStatus,
    /// Current operation or save result.
    message: String,
    /// Verbatim device identity.
    info: String,
    /// Failure retained for the error screen.
    error: Option<String>,
    /// Bounded raw history and derived traces.
    trace: TraceBuffer,
    /// Calculation selected by the terminal.
    mode: TraceMode,
    /// Monotonic renderer invalidation counter.
    generation: u64,
    /// Graceful cancellation requested by the terminal.
    stop_requested: bool,
    /// Visible trace captured for the next worker-side intermediate export.
    pending_save: Option<Vec<Sample>>,
    /// Save requested before the first complete sweep became available.
    save_after_first: bool,
}

/// Renderer-owned copy of one derived live trace.
pub(crate) struct LiveSnapshot {
    /// Monotonic state generation.
    pub(crate) generation: u64,
    /// Selected host-side calculation.
    pub(crate) mode: TraceMode,
    /// Complete sweeps incorporated in the calculation.
    pub(crate) sweeps: u64,
    /// Plot-ready `(MHz, dBm)` points.
    pub(crate) data: Vec<(f64, f64)>,
}

impl Default for LiveState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(LiveProgress {
            status: ScanStatus::Ready,
            message: String::new(),
            info: String::new(),
            error: None,
            trace: TraceBuffer::default(),
            mode: TraceMode::Average4,
            generation: 0,
            stop_requested: false,
            pending_save: None,
            save_after_first: false,
        })))
    }
}

impl ScanState {
    /// Recover telemetry after a poisoned lock rather than panic in error rendering.
    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reflect a library event without coupling the library to the terminal.
    fn report(&self, event: ScanEvent) {
        let mut state = self.lock();
        match event {
            ScanEvent::Message(message) => state.message = message.into(),
            ScanEvent::Progress(percent) => state.percent = usize::from(percent).min(99),
            ScanEvent::InstrumentInfo(info) => state.info = info,
        }
    }

    /// Clear results and errors before launching another worker.
    pub(crate) fn begin(&self) {
        let mut progress = self.lock();
        *progress = Progress {
            status: ScanStatus::Running,
            message: "Opening connection".into(),
            ..Progress::default()
        };
    }

    /// Current completion estimate; 100 means acquisition and export succeeded.
    pub(crate) fn progress(&self) -> usize {
        self.lock().percent
    }
    /// Current operation description.
    pub(crate) fn message(&self) -> String {
        self.lock().message.clone()
    }
    /// Device identification response.
    pub(crate) fn info(&self) -> String {
        self.lock().info.clone()
    }
    /// Current lifecycle state.
    pub(crate) fn status(&self) -> ScanStatus {
        self.lock().status
    }
    /// Error text, if any.
    pub(crate) fn error(&self) -> Option<String> {
        self.lock().error.clone()
    }
    /// Transfer completed samples once, avoiding per-frame copies of a large scan.
    pub(crate) fn take_samples(&self) -> Result<Vec<(f64, f64)>, UiError> {
        self.lock().samples.take().ok_or(UiError::NoReturnData)
    }
    /// Publish output and samples together after successful acquisition and export.
    pub(crate) fn complete(&self, filename: PathBuf, samples: Vec<(f64, f64)>) {
        let mut progress = self.lock();
        progress.message = format!("Saved {}", filename.display());
        progress.filename = Some(filename);
        progress.samples = Some(samples);
        progress.percent = 100;
        progress.status = ScanStatus::Complete;
    }

    /// Record failure and move to the error state together.
    pub(crate) fn fail(&self, error: impl std::fmt::Display) {
        log::error!("{error}");
        let mut state = self.lock();
        state.error = Some(error.to_string());
        state.status = ScanStatus::Failed;
    }
}

impl LiveState {
    /// Recover shared live state after a poisoned worker lock.
    fn lock(&self) -> MutexGuard<'_, LiveProgress> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reset all bounded history and control flags for a new live run.
    pub(crate) fn begin(&self) {
        let mut state = self.lock();
        *state = LiveProgress {
            status: ScanStatus::Running,
            message: "Opening live connection".into(),
            info: String::new(),
            error: None,
            trace: TraceBuffer::default(),
            mode: TraceMode::Average4,
            generation: state.generation.saturating_add(1),
            stop_requested: false,
            pending_save: None,
            save_after_first: false,
        };
    }

    /// Adapt core acquisition events without exposing the terminal to the core.
    fn report(&self, event: ScanEvent) {
        let mut state = self.lock();
        match event {
            ScanEvent::Message(message) => state.message = message.into(),
            ScanEvent::InstrumentInfo(info) => state.info = info,
            ScanEvent::Progress(_) => {}
        }
    }

    /// Incorporate one decoded complete sweep.
    fn publish(&self, samples: Vec<Sample>) -> Result<(), tiny_wb::live::LiveTraceError> {
        let mut state = self.lock();
        state.trace.push(samples)?;
        state.generation = state.generation.saturating_add(1);
        state.message = format!("Live capture — {} sweeps", state.trace.sweeps());
        Ok(())
    }

    /// Return a rendered trace only when samples or the selected mode changed.
    pub(crate) fn snapshot_after(&self, generation: u64) -> Option<LiveSnapshot> {
        let state = self.lock();
        if state.generation == generation {
            return None;
        }
        state.trace.trace(state.mode).map(|samples| LiveSnapshot {
            generation: state.generation,
            mode: state.mode,
            sweeps: state.trace.sweeps(),
            data: samples
                .into_iter()
                .map(|sample| (sample.frequency_mhz, sample.level_dbm))
                .collect(),
        })
    }

    /// Copy the selected calculation for worker-side CSV export.
    fn samples(&self) -> Option<Vec<Sample>> {
        let state = self.lock();
        state.trace.trace(state.mode)
    }

    /// Select a host-side calculation without discarding its retained history.
    pub(crate) fn set_mode(&self, mode: TraceMode) {
        let mut state = self.lock();
        if state.mode != mode {
            state.mode = mode;
            state.generation = state.generation.saturating_add(1);
        }
    }

    /// Current host-side calculation.
    pub(crate) fn mode(&self) -> TraceMode {
        self.lock().mode
    }

    /// Capture the visible trace for worker-side export after the current sweep.
    pub(crate) fn request_save(&self) {
        let mut state = self.lock();
        if let Some(samples) = state.trace.trace(state.mode) {
            state.pending_save = Some(samples);
            state.message = "Save requested for the visible buffer".into();
        } else {
            state.save_after_first = true;
            state.message = "Save requested; waiting for the first complete sweep".into();
        }
    }

    /// Consume one captured intermediate export, deferring an early request.
    fn take_save_request(&self) -> Option<Vec<Sample>> {
        let mut state = self.lock();
        if let Some(samples) = state.pending_save.take() {
            return Some(samples);
        }
        if state.save_after_first {
            if let Some(samples) = state.trace.trace(state.mode) {
                state.save_after_first = false;
                return Some(samples);
            }
        }
        None
    }

    /// Request cancellation after the current sweep completes.
    pub(crate) fn request_stop(&self) {
        let mut state = self.lock();
        state.stop_requested = true;
        state.message = "Stopping after the current sweep".into();
    }

    /// Whether the worker should refrain from starting another snapshot.
    fn should_stop(&self) -> bool {
        self.lock().stop_requested
    }

    /// Report a successful intermediate export without stopping capture.
    fn saved(&self, path: &std::path::Path) {
        self.lock().message = format!("Saved {}; live capture continues", path.display());
    }

    /// Publish final export and recovery completion together.
    fn complete(&self, path: Option<&std::path::Path>) {
        let mut state = self.lock();
        state.status = ScanStatus::Complete;
        state.message = path.map_or_else(
            || "Live capture stopped before the first complete sweep".into(),
            |path| format!("Saved {} and stopped live capture", path.display()),
        );
    }

    /// Record a worker failure for the terminal error screen.
    pub(crate) fn fail(&self, error: impl std::fmt::Display) {
        let mut state = self.lock();
        state.error = Some(error.to_string());
        state.status = ScanStatus::Failed;
    }

    /// Current live worker lifecycle.
    pub(crate) fn status(&self) -> ScanStatus {
        self.lock().status
    }

    /// Current live operation or save result.
    pub(crate) fn message(&self) -> String {
        self.lock().message.clone()
    }

    /// Device identification text received by the live worker.
    pub(crate) fn info(&self) -> String {
        self.lock().info.clone()
    }

    /// Live worker failure, when present.
    pub(crate) fn error(&self) -> Option<String> {
        self.lock().error.clone()
    }
}

/// Reopen once when complete identity replies remain empty or unrecognized.
///
/// Dropping the entire shell before opening again releases the port and its
/// buffered replies. These errors occur before any acquisition settings change;
/// transport errors and failures after configuration must never replay a scan.
fn with_identity_reconnect<T: Read + Write, R>(
    max_response_bytes: usize,
    mut open: impl FnMut() -> Result<T, ConnectionError>,
    mut operation: impl FnMut(&mut Shell<T>) -> Result<R, AcquisitionError>,
    mut on_retry: impl FnMut(),
) -> Result<R, AppError> {
    let first = {
        let mut shell = Shell::new(open()?, max_response_bytes);
        operation(&mut shell)
    };
    match first {
        Err(
            error @ (AcquisitionError::MissingIdentity
            | AcquisitionError::Device(DeviceError::Unknown)),
        ) => {
            log::warn!("instrument identification failed; reopening connection once: {error}");
            on_retry();
            let mut shell = Shell::new(open()?, max_response_bytes);
            Ok(operation(&mut shell)?)
        }
        result => Ok(result?),
    }
}

/// Acquire with one identity-only reconnect, then export valid samples.
pub(crate) fn run_scan(
    state: ScanState,
    request: ScanRequest,
    config: Config,
) -> Result<(), AppError> {
    state.begin();
    config.validate()?;
    request
        .validate()
        .map_err(tiny_wb::acquisition::AcquisitionError::from)?;
    let samples = with_identity_reconnect(
        config.serial.max_response_bytes,
        || open_connection(&config),
        |shell| {
            acquire(shell, &request, &config.scan, &config.serial, |event| {
                state.report(event)
            })
        },
        || {
            state.report(ScanEvent::Message(
                "Reconnecting after failed identification",
            ))
        },
    )?;
    state.report(ScanEvent::Message("Writing CSV"));
    let filename = save_csv(&samples, &config.output)?;
    state.complete(
        filename,
        samples
            .into_iter()
            .map(|sample| (sample.frequency_mhz, sample.level_dbm))
            .collect(),
    );
    Ok(())
}

/// Keep one connection configured while acquiring repeated complete snapshots.
pub(crate) fn run_live(
    state: LiveState,
    request: ScanRequest,
    config: Config,
) -> Result<(), AppError> {
    if state.status() == ScanStatus::Ready {
        state.begin();
    }
    config.validate()?;
    request
        .validate()
        .map_err(tiny_wb::acquisition::AcquisitionError::from)?;
    with_identity_reconnect(
        config.serial.max_response_bytes,
        || open_connection(&config),
        |shell| {
            acquire_live(
                shell,
                &request,
                &config.scan,
                &config.serial,
                || state.should_stop(),
                |samples| {
                    if let Err(error) = state.publish(samples) {
                        state.fail(error);
                        state.request_stop();
                        return;
                    }
                    if let Some(samples) = state.take_save_request() {
                        match save_csv(&samples, &config.output) {
                            Ok(path) => state.saved(&path),
                            Err(error) => {
                                state.fail(error);
                                state.request_stop();
                            }
                        }
                    }
                },
                |event| state.report(event),
            )
        },
        || {
            state.report(ScanEvent::Message(
                "Reconnecting after failed identification",
            ))
        },
    )?;
    if let Some(error) = state.error() {
        return Err(AppError::Live(error));
    }
    let saved = match state.samples() {
        Some(samples) => Some(save_csv(&samples, &config.output)?),
        None => None,
    };
    state.complete(saved.as_deref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        cell::{Cell, RefCell},
        io::{self, Cursor},
        net::TcpListener,
        rc::Rc,
        thread,
        time::{Duration, Instant},
    };

    use tiny_wb::{
        config::Backend,
        mock::{serve_connection, MockConfig},
    };

    struct IdentitySession {
        input: Cursor<Vec<u8>>,
        commands: Rc<RefCell<Vec<u8>>>,
        closed: Rc<Cell<bool>>,
    }

    impl Read for IdentitySession {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl Write for IdentitySession {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.commands.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for IdentitySession {
        fn drop(&mut self) {
            self.closed.set(true);
        }
    }

    #[test]
    fn single_and_live_scans_reconnect_only_after_unusable_identity() {
        for live in [false, true] {
            // None also checks that a healthy first connection is never replayed.
            for failed_reply in [None, Some(""), Some("stale startup text")] {
                let config = Config::from_toml(
                    "[scan]\nlegacy_pre_scan=false\n[serial]\ncommand_delay_ms=0\nsettle_delay_ms=0",
                ).unwrap();
                let request = ScanRequest {
                    start_hz: 280_000_000,
                    stop_hz: 320_000_000,
                    points: 2,
                    repeat: 1,
                };
                let closed = Rc::new(Cell::new(true));
                let commands = [
                    Rc::new(RefCell::new(Vec::new())),
                    Rc::new(RefCell::new(Vec::new())),
                ];
                let mut opened = 0;
                let mut retries = 0;
                let mut open = || {
                    assert!(
                        closed.replace(false),
                        "old port must close before reopening"
                    );
                    let input = if let Some(reply) = failed_reply.filter(|_| opened == 0) {
                        // Extra bytes are prefetched into the first shell's buffer.
                        // They must not leak into the fresh identity exchange.
                        format!("ch>{reply}ch>{reply}ch>{reply}ch>{reply}ch>leftover stale bytes")
                            .into_bytes()
                    } else {
                        b"ch>tinySA_v1.4-test\r\nch>tinySA\r\nch>repeat zero pause mode sweep scanraw resume\r\nch>ch>ch>128dBm\r\nch>ch>ch>ch>ch>ch>{x\0\x08x\0\x10}ch>ch>".to_vec()
                    };
                    let session = IdentitySession {
                        input: Cursor::new(input),
                        commands: commands[opened].clone(),
                        closed: closed.clone(),
                    };
                    opened += 1;
                    Ok(session)
                };
                let sweeps = Cell::new(0);
                let result = with_identity_reconnect(
                    1024,
                    &mut open,
                    |shell| {
                        if live {
                            acquire_live(
                                shell,
                                &request,
                                &config.scan,
                                &config.serial,
                                || sweeps.get() == 1,
                                |samples| {
                                    assert_eq!(samples.len(), 2);
                                    sweeps.set(sweeps.get() + 1);
                                },
                                |_| {},
                            )
                            .map(|_| ())
                        } else {
                            acquire(shell, &request, &config.scan, &config.serial, |_| {})
                                .map(|samples| assert_eq!(samples.len(), 2))
                        }
                    },
                    || retries += 1,
                );
                assert!(result.is_ok(), "{result:?}");
                assert_eq!(opened, 1 + usize::from(failed_reply.is_some()));
                assert_eq!(retries, usize::from(failed_reply.is_some()));
                assert!(closed.get());
                if failed_reply.is_some() {
                    assert_eq!(&*commands[0].borrow(), b"version\rinfo\rversion\rinfo\r");
                }
                let successful = String::from_utf8(commands[opened - 1].borrow().clone()).unwrap();
                assert_eq!(successful.matches("scanraw ").count(), 1);
                assert!(successful.ends_with("resume\r"));
            }
        }
    }

    #[test]
    fn reconnect_is_bounded_and_other_acquisition_errors_are_not_retried() {
        for (error, expected_opens) in [
            (AcquisitionError::MissingIdentity, 2),
            (AcquisitionError::Device(DeviceError::Unknown), 2),
            (AcquisitionError::Device(DeviceError::Conflicting), 1),
            (
                AcquisitionError::Device(DeviceError::ConflictingRevision),
                1,
            ),
            (
                AcquisitionError::Transport(io::Error::from(io::ErrorKind::TimedOut).into()),
                1,
            ),
            (AcquisitionError::MissingZeroReadback { requested: 128 }, 1),
            (
                AcquisitionError::Protocol(tiny_wb::protocol::ProtocolError::MissingEnd),
                1,
            ),
        ] {
            let mut opened = 0;
            let mut error = Some(error);
            let result = with_identity_reconnect(
                256,
                || {
                    opened += 1;
                    Ok(Cursor::new(Vec::<u8>::new()))
                },
                |_| {
                    Err::<(), _>(
                        error
                            .take()
                            .unwrap_or(AcquisitionError::Device(DeviceError::Unknown)),
                    )
                },
                || {},
            );
            assert!(matches!(result, Err(AppError::Acquisition(_))));
            assert_eq!(opened, expected_opens);
        }
    }

    #[test]
    fn connection_open_errors_are_propagated_without_another_attempt() {
        for fail_on in [1, 2] {
            let mut opened = 0;
            let mut operations = 0;
            let result = with_identity_reconnect(
                256,
                || {
                    opened += 1;
                    if opened == fail_on {
                        Err(ConnectionError::Tcp(io::ErrorKind::PermissionDenied.into()))
                    } else {
                        Ok(Cursor::new(Vec::<u8>::new()))
                    }
                },
                |_| {
                    operations += 1;
                    Err::<(), _>(AcquisitionError::MissingIdentity)
                },
                || {},
            );
            assert!(matches!(result, Err(AppError::Connection(_))));
            assert_eq!(opened, fail_on);
            assert_eq!(operations, fail_on - 1);
        }
    }

    #[test]
    fn shared_progress_and_errors_are_visible_across_clones() {
        let state = ScanState::default();
        let worker = state.clone();
        worker.report(ScanEvent::Progress(100));
        assert_eq!(state.progress(), 99, "export must finish before 100%");
        worker.report(ScanEvent::InstrumentInfo("Basic".into()));
        assert_eq!(state.info(), "Basic");
        worker.fail("disconnected");
        assert_eq!(state.status(), ScanStatus::Failed);
        assert_eq!(state.error().as_deref(), Some("disconnected"));
    }

    #[test]
    fn completed_samples_are_transferred_once() {
        let state = ScanState::default();
        state.lock().samples = Some(vec![(240.0, -64.0)]);
        assert_eq!(state.take_samples().unwrap(), vec![(240.0, -64.0)]);
        assert!(state.take_samples().is_err());
    }

    #[test]
    fn beginning_another_scan_clears_previous_results_and_errors() {
        let state = ScanState::default();
        state.complete(PathBuf::from("old.csv"), vec![(240.0, -64.0)]);
        state.fail("old failure");

        state.begin();

        assert_eq!(state.status(), ScanStatus::Running);
        assert_eq!(state.progress(), 0);
        assert_eq!(state.message(), "Opening connection");
        assert!(state.info().is_empty());
        assert!(state.error().is_none());
        assert!(state.take_samples().is_err());
    }

    #[test]
    fn live_state_publishes_modes_and_control_requests_coherently() {
        let state = LiveState::default();
        state.begin();
        state
            .publish(vec![Sample {
                frequency_mhz: 240.0,
                level_dbm: -64.0,
            }])
            .unwrap();
        let snapshot = state.snapshot_after(0).unwrap();
        assert_eq!(snapshot.mode, TraceMode::Average4);
        assert_eq!(snapshot.sweeps, 1);
        assert_eq!(snapshot.data, vec![(240.0, -64.0)]);
        assert!(state.snapshot_after(snapshot.generation).is_none());

        state.set_mode(TraceMode::MaxHold);
        assert_eq!(state.mode(), TraceMode::MaxHold);
        assert!(state.snapshot_after(snapshot.generation).is_some());
        state.request_save();
        state
            .publish(vec![Sample {
                frequency_mhz: 240.0,
                level_dbm: -20.0,
            }])
            .unwrap();
        assert_eq!(state.take_save_request().unwrap()[0].level_dbm, -64.0);
        assert!(state.take_save_request().is_none());
        state.request_stop();
        assert!(state.should_stop());
    }

    fn mock_tcp_config(output_name: &str) -> (Config, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = Config::default();
        config.connection.backend = Backend::Tcp;
        config.connection.tcp_address = listener.local_addr().unwrap().to_string();
        config.serial.command_delay_ms = 0;
        config.serial.settle_delay_ms = 0;
        config.serial.scan_delay_ms = 0;
        config.scan.legacy_pre_scan = false;
        config.output.directory =
            std::env::temp_dir().join(format!("tiny-wb-{output_name}-{}", std::process::id()));
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            serve_connection(socket, MockConfig::default()).unwrap();
        });
        (config, server)
    }

    fn mock_request() -> ScanRequest {
        ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 360_000_000,
            points: 4,
            repeat: 1,
        }
    }

    #[test]
    fn scan_worker_acquires_exports_and_publishes_completion() {
        let (config, server) = mock_tcp_config("scan-worker");
        let output = config.output.directory.clone();
        let state = ScanState::default();

        run_scan(state.clone(), mock_request(), config).unwrap();

        assert_eq!(state.status(), ScanStatus::Complete);
        assert_eq!(state.progress(), 100);
        assert_eq!(state.take_samples().unwrap().len(), 4);
        assert!(state.message().starts_with("Saved "));
        server.join().unwrap();
        std::fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn live_worker_honors_deferred_save_and_graceful_stop() {
        let (config, server) = mock_tcp_config("live-worker");
        let output = config.output.directory.clone();
        let state = LiveState::default();
        state.begin();
        state.report(ScanEvent::InstrumentInfo("mock identity".into()));
        assert_eq!(state.info(), "mock identity");
        state.request_save();

        let controller = state.clone();
        let stop = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while controller.snapshot_after(0).is_none() {
                assert!(
                    Instant::now() < deadline,
                    "live worker produced no snapshot"
                );
                thread::yield_now();
            }
            controller.request_stop();
        });

        run_live(state.clone(), mock_request(), config).unwrap();

        stop.join().unwrap();
        server.join().unwrap();
        assert_eq!(state.status(), ScanStatus::Complete);
        assert!(state.error().is_none());
        assert!(state.message().contains("stopped live capture"));
        assert!(std::fs::read_dir(&output).unwrap().count() >= 2);
        std::fs::remove_dir_all(output).unwrap();
    }
}
