//! Serial worker adapter and shared progress state for the terminal application.
//!
//! The worker opens the device and exports completed samples. Protocol parsing
//! and orchestration stay in the library; no core type knows about UI widgets.

use crate::error::{AppError, UiError};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use tiny_wb::{
    acquisition::{acquire, acquire_live, ScanEvent},
    config::Config,
    connection::open_connection,
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

/// Open one port, acquire through the reusable core, then export valid samples.
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
    let port = open_connection(&config)?;
    let mut shell = Shell::new(port, config.serial.max_response_bytes);
    let samples = acquire(
        &mut shell,
        &request,
        &config.scan,
        &config.serial,
        |event| state.report(event),
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
    let port = open_connection(&config)?;
    let mut shell = Shell::new(port, config.serial.max_response_bytes);
    let callback_state = state.clone();
    let output = config.output.clone();
    acquire_live(
        &mut shell,
        &request,
        &config.scan,
        &config.serial,
        || state.should_stop(),
        move |samples| {
            if let Err(error) = callback_state.publish(samples) {
                callback_state.fail(error);
                callback_state.request_stop();
                return;
            }
            if let Some(samples) = callback_state.take_save_request() {
                match save_csv(&samples, &output) {
                    Ok(path) => callback_state.saved(&path),
                    Err(error) => {
                        callback_state.fail(error);
                        callback_state.request_stop();
                    }
                }
            }
        },
        |event| state.report(event),
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
}
