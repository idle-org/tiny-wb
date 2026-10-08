//! UI state, bounded selections and rendering widget storage.
use color_eyre::Result;
use ratatui::{
    style::Color,
    widgets::{LineGauge, List, ListState},
};
use serialport::SerialPortInfo;
use std::path::PathBuf;
use strum::EnumCount;
use strum::FromRepr;

use crate::{
    error::{AppError, UiError},
    scan::ScanState,
};
use tiny_wb::{config::Config, protocol::ScanRequest};

/// Color of a selected border
pub(crate) const SELECTED_COLOR: Color = Color::White;
/// Color of an unselected item
pub(crate) const UNSELECTED_COLOR: Color = Color::DarkGray;

/// This struct holds the current state of the app.
#[derive(Debug)]
pub(crate) struct App {
    /// Whether the user requested exit.
    pub(crate) should_quit: bool,
    /// Selectable baud rates.
    pub(crate) baudrate: BaudRateList,
    /// Enumerated serial ports.
    pub(crate) port_list: PortList,
    /// Current screen and focused field.
    pub(crate) flow_state: AppState,
    /// Cached widgets rebuilt during screen initialization.
    pub(crate) menu_items: MenuItems,
    /// Application version shown above the controls.
    pub(crate) status_message: String,
    /// Requested start frequency, in MHz.
    pub(crate) start_frequency: NumericInput,
    /// Requested stop frequency, in MHz.
    pub(crate) stop_frequency: NumericInput,
    /// Requested number of samples.
    pub(crate) points: NumericInput,
    /// Measurements per point.
    pub(crate) repeat: NumericInput,
    /// Shared acquisition telemetry; request settings are passed separately.
    pub(crate) scan_state: ScanState,
    /// Validated settings retained when the selection is saved.
    pub(crate) config: Config,
    /// Explicit settings path selected at startup.
    pub(crate) config_path: PathBuf,
    /// Last launched request and settings, retained for one-key repetition.
    pub(crate) last_scan: Option<(ScanRequest, Config)>,
    /// Whether this session may acquire, as opposed to viewing a standalone CSV.
    pub(crate) can_acquire: bool,
    /// Plot currently shown by the renderer; a collection boundary for future tabs.
    pub(crate) plot: Option<Plot>,
}

/// One named plot and its independently navigable frequency viewport.
#[derive(Debug)]
pub(crate) struct Plot {
    /// User-facing source name.
    pub(crate) label: String,
    /// Samples in MHz and dBm, ordered by frequency.
    pub(crate) data: Vec<(f64, f64)>,
    /// Current and complete horizontal bounds.
    pub(crate) viewport: FrequencyViewport,
}

impl Plot {
    /// Construct a plot whose initial viewport contains all samples.
    pub(crate) fn new(label: impl Into<String>, data: Vec<(f64, f64)>) -> Self {
        let viewport = FrequencyViewport::from_samples(&data);
        Self {
            label: label.into(),
            data,
            viewport,
        }
    }
}

/// Horizontal chart bounds with zoom and pan clamped to the source data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrequencyViewport {
    /// Complete lower bound, in MHz.
    full_min: f64,
    /// Complete upper bound, in MHz.
    full_max: f64,
    /// Visible lower bound, in MHz.
    visible_min: f64,
    /// Visible upper bound, in MHz.
    visible_max: f64,
    /// Smallest permitted visible span.
    min_span: f64,
}

impl FrequencyViewport {
    /// Derive safe increasing chart bounds from an ordered sample set.
    fn from_samples(samples: &[(f64, f64)]) -> Self {
        let first = samples.first().map_or(0.0, |sample| sample.0);
        let last = samples.last().map_or(first, |sample| sample.0);
        let (full_min, full_max) = if first < last {
            (first, last)
        } else {
            let padding = (first.abs() * 1e-6).max(0.001);
            (first - padding, first + padding)
        };
        let span = full_max - full_min;
        let sample_spacing = samples
            .windows(2)
            .filter_map(|pair| {
                let spacing = pair[1].0 - pair[0].0;
                (spacing > 0.0).then_some(spacing)
            })
            .min_by(f64::total_cmp);
        Self {
            full_min,
            full_max,
            visible_min: full_min,
            visible_max: full_max,
            min_span: sample_spacing.unwrap_or(span).min(span).max(f64::EPSILON),
        }
    }

    /// Current `[minimum, maximum]` bounds in MHz.
    pub(crate) fn bounds(self) -> [f64; 2] {
        [self.visible_min, self.visible_max]
    }

    /// Halve the visible span around its center, down to the resolution floor.
    pub(crate) fn zoom_in(&mut self) {
        self.set_span(((self.visible_max - self.visible_min) / 2.0).max(self.min_span));
    }

    /// Double the visible span around its center, up to the complete range.
    pub(crate) fn zoom_out(&mut self) {
        self.set_span(((self.visible_max - self.visible_min) * 2.0).min(self.full_span()));
    }

    /// Move by one quarter of the visible span and clamp at the data edge.
    pub(crate) fn pan(&mut self, direction: InputDirection) {
        let span = self.visible_max - self.visible_min;
        let shift = match direction {
            InputDirection::Left => -span / 4.0,
            InputDirection::Right => span / 4.0,
            _ => return,
        };
        let new_min = (self.visible_min + shift).clamp(self.full_min, self.full_max - span);
        self.visible_min = new_min;
        self.visible_max = new_min + span;
    }

    /// Restore the complete source range.
    pub(crate) fn reset(&mut self) {
        self.visible_min = self.full_min;
        self.visible_max = self.full_max;
    }

    /// Update the visible span while retaining the current center when possible.
    fn set_span(&mut self, span: f64) {
        let span = span.clamp(self.min_span, self.full_span());
        let center = (self.visible_min + self.visible_max) / 2.0;
        let new_min = (center - span / 2.0).clamp(self.full_min, self.full_max - span);
        self.visible_min = new_min;
        self.visible_max = new_min + span;
    }

    /// Width of the complete frequency range.
    fn full_span(self) -> f64 {
        self.full_max - self.full_min
    }
}

#[derive(Debug)]
/// Numeric input state with bounded keyboard increments.
pub(crate) struct NumericInput {
    /// Current bounded value.
    value: usize,
    /// Keyboard increment.
    increment: usize,
    /// Inclusive lower bound.
    min_clamp: usize,
    /// Inclusive upper bound.
    max_clamp: usize,
    /// Digits being entered directly; `None` retains step-based behavior.
    edit_buffer: Option<String>,
    /// Whether the current buffer failed bounds or integer validation.
    invalid_edit: bool,
}

impl NumericInput {
    /// Change the value depending on selection
    pub(crate) fn change_value(&mut self, direction: InputDirection) {
        match direction {
            InputDirection::Up => {
                self.value = self
                    .value
                    .saturating_add(self.increment)
                    .clamp(self.min_clamp, self.max_clamp)
            }
            InputDirection::Down => {
                self.value = self
                    .value
                    .saturating_sub(self.increment)
                    .clamp(self.min_clamp, self.max_clamp)
            }
            _ => (),
        }
    }

    /// Create bounded input, clamping a supplied value to validated bounds.
    pub(crate) fn new(value: usize, increment: usize, min_clamp: usize, max_clamp: usize) -> Self {
        Self {
            value: value.clamp(min_clamp, max_clamp),
            increment,
            max_clamp,
            min_clamp,
            edit_buffer: None,
            invalid_edit: false,
        }
    }

    /// Read the current value.
    pub(crate) fn value(&self) -> usize {
        self.value
    }

    /// Start or extend direct decimal entry without changing the committed value.
    pub(crate) fn push_digit(&mut self, digit: char) {
        if !digit.is_ascii_digit() {
            return;
        }
        let buffer = self.edit_buffer.get_or_insert_with(String::new);
        if buffer.len() < 20 {
            buffer.push(digit);
        }
        self.invalid_edit = false;
    }

    /// Remove the last typed digit while retaining edit mode.
    pub(crate) fn backspace(&mut self) {
        if let Some(buffer) = self.edit_buffer.as_mut() {
            buffer.pop();
            self.invalid_edit = false;
        }
    }

    /// Commit direct entry when it is an integer inside the field's bounds.
    pub(crate) fn commit_edit(&mut self) -> bool {
        let parsed = self
            .edit_buffer
            .as_deref()
            .filter(|buffer| !buffer.is_empty())
            .and_then(|buffer| buffer.parse::<usize>().ok());
        if let Some(value) =
            parsed.filter(|value| (self.min_clamp..=self.max_clamp).contains(value))
        {
            self.value = value;
            self.edit_buffer = None;
            self.invalid_edit = false;
            true
        } else {
            self.invalid_edit = true;
            false
        }
    }

    /// Abandon direct entry and restore the previously committed value.
    pub(crate) fn cancel_edit(&mut self) {
        self.edit_buffer = None;
        self.invalid_edit = false;
    }

    /// Whether keystrokes currently belong to the inline editor.
    pub(crate) fn is_editing(&self) -> bool {
        self.edit_buffer.is_some()
    }
}

#[derive(Debug)]
/// The inner state of the app, useful for display and keystrokes
pub(crate) enum AppState {
    /// Initial config, baudrate and port
    SelectingConnection(SelectedConnection),
    /// Final config, frequencies and points
    SelectingScan(SelectedScan),
    /// The end window for confirmation
    Complete,
    /// Acquisition and export progress.
    Scanning,
    /// Error window
    Error,
}

impl Default for AppState {
    fn default() -> Self {
        Self::SelectingConnection(SelectedConnection::default())
    }
}

/// The selected item in the config
#[derive(Debug, Default, EnumCount, FromRepr, Clone, Copy)]
pub(crate) enum SelectedConnection {
    /// Selects the ports
    #[default]
    Port,
    /// Selects the baudrate
    Baudrate,
}

/// The selected item in the final menu
#[derive(Debug, Default, EnumCount, FromRepr, Copy, Clone)]
pub(crate) enum SelectedScan {
    /// Selects the start frequency
    #[default]
    StartFreq,
    /// Selects the final frequency
    FinalFreq,
    /// Selects the number of points
    Points,
    /// Selects the repeat
    Repeat,
}

/// Configured baud rates and their selected index.
#[derive(Debug)]
pub(crate) struct BaudRateList {
    /// Choices loaded from validated settings.
    pub(crate) baudrates: Vec<u32>,
    /// Selection state used by Ratatui.
    pub(crate) state: ListState,
}
impl BaudRateList {
    /// Construct choices with the configured rate selected.
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            baudrates: config.serial.baudrates.clone(),
            state: ListState::default().with_selected(
                config
                    .serial
                    .baudrates
                    .iter()
                    .position(|rate| *rate == config.baudrate),
            ),
        }
    }
    /// Format values for the list widget.
    pub(crate) fn to_vecstr(&self) -> Vec<String> {
        self.baudrates.iter().map(u32::to_string).collect()
    }
    /// Move without selecting an index beyond the configured choices.
    pub(crate) fn change_cursor(&mut self, direction: InputDirection) {
        move_list_selection(&mut self.state, self.baudrates.len(), direction);
    }
    /// Resolve the selected value safely even if the list changed.
    pub(crate) fn selected_baudrate(&self) -> Result<u32, UiError> {
        self.state
            .selected()
            .and_then(|index| self.baudrates.get(index))
            .copied()
            .ok_or(UiError::MenuSelection)
    }
}

/// A wrapper over the list of ports
#[derive(Debug)]
pub(crate) struct PortList {
    /// The list of found ports
    pub(crate) ports: Vec<String>,
    /// The inner state of the widget
    pub(crate) state: ListState,
}

impl PortList {
    /// Make a port list from a serial reading
    pub(crate) fn from_serial(info: Vec<SerialPortInfo>, config: &Config) -> Self {
        let ports: Vec<String> = info.into_iter().map(|port| port.port_name).collect();
        let selected = find_best_port(&ports, config);
        Self {
            ports,
            state: ListState::default().with_selected(selected),
        }
    }

    /// Convert the list of ports
    pub(crate) fn to_vecstr(&self) -> Vec<String> {
        self.ports.clone()
    }

    /// Change the selected item
    pub(crate) fn change_cursor(&mut self, direction: InputDirection) {
        move_list_selection(&mut self.state, self.ports.len(), direction);
    }

    /// Returns the final selected port
    pub(crate) fn selected_port(&self) -> Result<String, UiError> {
        self.state
            .selected()
            .and_then(|index| self.ports.get(index))
            .cloned()
            .ok_or(UiError::NoPortSelected)
    }
}

#[derive(Debug, Default)]
/// Cached widgets; none borrow mutable application data.
pub(crate) struct MenuItems {
    /// Cached list widget.
    pub(crate) portlist: Option<List<'static>>,
    /// Cached list widget.
    pub(crate) baudlist: Option<List<'static>>,
    /// The progress bar
    pub(crate) progress_bar: Option<LineGauge<'static>>,
}

/// Prefer the saved device, then configured candidates, then the first port.
fn find_best_port(ports: &[String], config: &Config) -> Option<usize> {
    std::iter::once(&config.port)
        .chain(config.serial.preferred_ports.iter())
        .find_map(|candidate| ports.iter().position(|port| port == candidate))
        .or_else(|| (!ports.is_empty()).then_some(0))
}

/// Keep selection valid without relying on a later draw to clamp its index.
fn move_list_selection(state: &mut ListState, length: usize, direction: InputDirection) {
    if length == 0 {
        state.select(None);
        return;
    }
    let current = state.selected().unwrap_or(0).min(length - 1);
    let next = match direction {
        InputDirection::Up => current.saturating_sub(1),
        InputDirection::Down => current.saturating_add(1).min(length - 1),
        _ => current,
    };
    state.select(Some(next));
}

impl App {
    /// Build validated settings from the current selection, preserving all sections.
    pub(crate) fn selected_config(&self) -> Result<Config, UiError> {
        let config = Config {
            port: self.port_list.selected_port()?,
            baudrate: self.baudrate.selected_baudrate()?,
            start_frequency: self.start_frequency.value(),
            stop_frequency: self.stop_frequency.value(),
            points: self.points.value(),
            repeat: self.repeat.value(),
            ..self.config.clone()
        };
        config.validate().map_err(AppError::from)?;
        Ok(config)
    }
}

/// Navigation intent independent of terminal key codes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InputDirection {
    /// Increase a numeric value or move toward the first list item.
    Up,
    /// Increase a list index or decrease a numeric value.
    Down,
    /// Select the next field.
    Right,
    /// Select the previous field.
    Left,
}

impl std::fmt::Display for NumericInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(buffer) = &self.edit_buffer {
            if self.invalid_edit {
                write!(
                    formatter,
                    "{}_ (enter {}–{})",
                    buffer, self.min_clamp, self.max_clamp
                )
            } else if buffer.is_empty() {
                formatter.write_str("_")
            } else {
                write!(formatter, "{buffer}_")
            }
        } else {
            self.value.fmt(formatter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_input_clamps_and_saturates_without_overflow() {
        let mut field = NumericInput::new(999, 10, 0, 100);
        assert_eq!(field.value(), 100);
        field.change_value(InputDirection::Up);
        assert_eq!(field.value(), 100);
        field.change_value(InputDirection::Down);
        assert_eq!(field.value(), 90);
        let mut field = NumericInput::new(usize::MAX - 1, 10, 0, usize::MAX);
        field.change_value(InputDirection::Up);
        assert_eq!(field.value(), usize::MAX);
        let mut field = NumericInput::new(1, 10, 0, 100);
        field.change_value(InputDirection::Down);
        assert_eq!(field.value(), 0);
    }

    #[test]
    fn numeric_input_edits_commits_and_cancels_typed_values() {
        let mut field = NumericInput::new(240, 10, 0, 5_400);
        for digit in "1234".chars() {
            field.push_digit(digit);
        }
        assert!(field.is_editing());
        assert_eq!(field.to_string(), "1234_");
        assert!(field.commit_edit());
        assert_eq!(field.value(), 1234);

        for digit in "9999".chars() {
            field.push_digit(digit);
        }
        assert!(!field.commit_edit());
        assert_eq!(field.value(), 1234);
        assert!(field.to_string().contains("enter 0–5400"));
        field.backspace();
        field.cancel_edit();
        assert_eq!(field.to_string(), "1234");
    }

    #[test]
    fn viewport_zooms_pans_clamps_and_resets() {
        let mut viewport = FrequencyViewport::from_samples(&[
            (100.0, -1.0),
            (125.0, -1.5),
            (150.0, -2.0),
            (175.0, -1.5),
            (200.0, -1.0),
        ]);
        assert_eq!(viewport.bounds(), [100.0, 200.0]);
        viewport.zoom_in();
        assert_eq!(viewport.bounds(), [125.0, 175.0]);
        viewport.pan(InputDirection::Right);
        assert_eq!(viewport.bounds(), [137.5, 187.5]);
        for _ in 0..20 {
            viewport.pan(InputDirection::Right);
        }
        assert_eq!(viewport.bounds(), [150.0, 200.0]);
        viewport.zoom_out();
        assert_eq!(viewport.bounds(), [100.0, 200.0]);
        viewport.reset();
        assert_eq!(viewport.bounds(), [100.0, 200.0]);
    }

    #[test]
    fn maximum_zoom_uses_the_finest_positive_sample_spacing() {
        let mut viewport = FrequencyViewport::from_samples(&[
            (100.0, -1.0),
            (100.0, -2.0),
            (100.01, -3.0),
            (100.02, -4.0),
            (200.0, -5.0),
        ]);
        for _ in 0..32 {
            viewport.zoom_in();
        }
        let [minimum, maximum] = viewport.bounds();
        assert!(((maximum - minimum) - 0.01).abs() < 1e-12);
    }

    #[test]
    fn viewport_for_one_sample_still_has_increasing_bounds() {
        let viewport = FrequencyViewport::from_samples(&[(240.0, -64.0)]);
        let [minimum, maximum] = viewport.bounds();
        assert!(minimum < 240.0 && maximum > 240.0);
    }

    #[test]
    fn empty_and_stale_port_selections_return_errors() {
        let mut list = PortList::from_serial(Vec::new(), &Config::default());
        list.change_cursor(InputDirection::Down);
        assert!(list.state.selected().is_none());
        assert!(matches!(list.selected_port(), Err(UiError::NoPortSelected)));
        list.state.select(Some(100));
        assert!(list.selected_port().is_err());
    }

    #[test]
    fn saved_port_precedes_preferences_and_unavailable_ports_fall_back() {
        let mut config = Config::default();
        let ports = vec!["other".into(), "COM3".into(), "saved".into()];
        assert_eq!(find_best_port(&ports, &config), Some(1));
        config.port = "saved".into();
        assert_eq!(find_best_port(&ports, &config), Some(2));
        assert_eq!(find_best_port(&["other".into()], &config), Some(0));
        assert_eq!(find_best_port(&[], &config), None);
    }

    #[test]
    fn custom_baud_choices_are_bounded_before_rendering() {
        let config =
            Config::from_toml("baudrate = 9600\n[serial]\nbaudrates = [9600, 19200]").unwrap();
        let mut list = BaudRateList::new(&config);
        assert_eq!(list.selected_baudrate().unwrap(), 9600);
        for _ in 0..100 {
            list.change_cursor(InputDirection::Down);
        }
        assert_eq!(list.selected_baudrate().unwrap(), 19200);
        for _ in 0..100 {
            list.change_cursor(InputDirection::Up);
        }
        assert_eq!(list.selected_baudrate().unwrap(), 9600);
    }
}
