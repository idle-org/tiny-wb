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
    acquisition::{acquire, ScanEvent},
    config::Config,
    connection::open_connection,
    export::save_csv,
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
}
