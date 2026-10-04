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
use tiny_wb::config::Config;

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
    /// Completed samples owned by the UI, in MHz and dBm.
    pub(crate) return_data: Vec<(f64, f64)>,
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
        }
    }

    /// Read the current value.
    pub(crate) fn value(&self) -> usize {
        self.value
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
        self.value.fmt(formatter)
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
