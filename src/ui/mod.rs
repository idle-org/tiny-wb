//! Terminal control flow: initialize widgets, handle input and start workers.

mod state;
mod view;

use crate::{
    error::{AppError, UiError},
    scan::{self, ScanState, ScanStatus},
};
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    style::{Color, Modifier, Style},
    widgets::{Block, LineGauge, List},
    DefaultTerminal,
};
use serialport::SerialPortInfo;
pub(crate) use state::App;
use state::{
    AppState, BaudRateList, InputDirection, MenuItems, NumericInput, Plot, PortList,
    SelectedConnection, SelectedScan,
};
use std::{path::PathBuf, thread, time::Duration};
use strum::EnumCount;
use tiny_wb::{config::Config, protocol::ScanRequest};

impl App {
    /// Construct UI state from already validated settings; no file I/O here.
    pub(crate) fn new(info: Vec<SerialPortInfo>, config: Config, config_path: PathBuf) -> Self {
        Self {
            should_quit: false,
            baudrate: BaudRateList::new(&config),
            port_list: PortList::from_serial(info, &config),
            flow_state: AppState::default(),
            status_message: String::new(),
            menu_items: MenuItems::default(),
            start_frequency: NumericInput::new(
                config.start_frequency,
                config.scan.frequency_step_mhz,
                0,
                5_400,
            ),
            stop_frequency: NumericInput::new(
                config.stop_frequency,
                config.scan.frequency_step_mhz,
                10,
                5_400,
            ),
            points: NumericInput::new(
                config.points,
                config.scan.points_step,
                config.scan.min_points,
                config.scan.max_points,
            ),
            repeat: NumericInput::new(config.repeat, 1, 1, config.scan.max_repeat),
            scan_state: ScanState::default(),
            plot: None,
            last_scan: None,
            can_acquire: true,
            config,
            config_path,
        }
    }

    /// Construct the renderer directly from an already parsed CSV plot.
    pub(crate) fn viewer(config: Config, source: PathBuf, data: Vec<(f64, f64)>) -> Self {
        let mut app = Self::new(Vec::new(), config, PathBuf::new());
        app.flow_state = AppState::Complete;
        app.plot = Some(Plot::new(source.display().to_string(), data));
        app.can_acquire = false;
        app
    }

    /// Drive events and rendering; worker state is reconciled before each frame.
    pub(crate) fn run(
        &mut self,
        mut terminal: DefaultTerminal,
    ) -> Result<(), color_eyre::eyre::Error> {
        self.init_config()?;
        loop {
            if let Err(error) = self.refresh_scan() {
                self.scan_state.fail(error);
                self.flow_state = AppState::Error;
            }
            let mut draw_error = None;
            terminal.draw(|frame| {
                draw_error = self.draw(frame).err();
            })?;
            if let Some(error) = draw_error {
                return Err(error.into());
            }
            let timeout = Duration::from_millis(self.config.ui.poll_interval_ms);
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if let Err(e) = self.handle_start_keys(key) {
                        self.scan_state.fail(e);
                        self.flow_state = AppState::Error;
                    };
                };
                if self.should_quit {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Initialization of the config window
    fn init_config(&mut self) -> Result<(), UiError> {
        self.menu_items.portlist = Some(
            List::new(self.port_list.to_vecstr())
                .block(Block::bordered().title("COM Ports"))
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::ITALIC)
                        .bg(Color::Blue),
                )
                .repeat_highlight_symbol(true),
        );
        self.menu_items.baudlist = Some(
            List::new(self.baudrate.to_vecstr())
                .block(Block::bordered().title("Baudrates"))
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::ITALIC)
                        .bg(Color::Blue),
                )
                .repeat_highlight_symbol(true),
        );
        self.status_message = format!(
            "TinyWB version {}-{}, by the Dandaedre 🄯 2024",
            env!("CARGO_PKG_VERSION"),
            env!("GIT_HASH")
        );
        self.update_color()
    }

    /// Initialize the progress window
    fn init_progress(&mut self) -> Result<(), UiError> {
        self.menu_items.progress_bar = Some(
            LineGauge::default()
                .filled_style(Style::default().fg(Color::Blue))
                .unfilled_style(Style::default().fg(Color::Red))
                .label("Progress:"),
        );
        Ok(())
    }

    /// Map press events to navigation; releases and repeats do not mutate state.
    fn handle_start_keys(&mut self, key: KeyEvent) -> Result<(), UiError> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        if matches!(self.flow_state, AppState::Complete) {
            return self.handle_plot_key(key.code);
        }
        if self.handle_numeric_editor_key(key.code) {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.change_cursor_position(InputDirection::Down),
            KeyCode::Char('k') | KeyCode::Up => self.change_cursor_position(InputDirection::Up),
            KeyCode::Char('h') | KeyCode::Left => self.next_menu(InputDirection::Left)?,
            KeyCode::Char('l') | KeyCode::Right => self.next_menu(InputDirection::Right)?,
            KeyCode::Enter => match self.flow_state {
                AppState::Scanning => (),
                _ => self.next_step()?,
            },
            _ => {}
        }
        Ok(())
    }

    /// Give an active direct-entry editor exclusive use of keys and start one on a digit.
    fn handle_numeric_editor_key(&mut self, code: KeyCode) -> bool {
        let input = match self.flow_state {
            AppState::SelectingScan(SelectedScan::StartFreq) => &mut self.start_frequency,
            AppState::SelectingScan(SelectedScan::FinalFreq) => &mut self.stop_frequency,
            AppState::SelectingScan(SelectedScan::Points) => &mut self.points,
            _ => return false,
        };
        if input.is_editing() {
            match code {
                KeyCode::Char(digit) if digit.is_ascii_digit() => input.push_digit(digit),
                KeyCode::Backspace => input.backspace(),
                KeyCode::Enter => {
                    input.commit_edit();
                }
                KeyCode::Esc => input.cancel_edit(),
                _ => {}
            }
            true
        } else {
            match code {
                KeyCode::Char(digit) if digit.is_ascii_digit() => {
                    input.push_digit(digit);
                    true
                }
                _ => false,
            }
        }
    }

    /// Apply configured navigation keys on the plot screen.
    fn handle_plot_key(&mut self, code: KeyCode) -> Result<(), UiError> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.should_quit = true;
                return Ok(());
            }
            KeyCode::Char('r') if self.can_acquire => {
                self.repeat_last_scan()?;
                return Ok(());
            }
            KeyCode::Char('n') if self.can_acquire => {
                self.plot = None;
                self.flow_state = AppState::SelectingScan(SelectedScan::default());
                return Ok(());
            }
            _ => {}
        }
        let bindings = &self.config.ui.bindings;
        let action = if binding_matches(&code, &bindings.zoom_in) {
            Some(PlotAction::ZoomIn)
        } else if binding_matches(&code, &bindings.zoom_out) {
            Some(PlotAction::ZoomOut)
        } else if binding_matches(&code, &bindings.pan_left) {
            Some(PlotAction::PanLeft)
        } else if binding_matches(&code, &bindings.pan_right) {
            Some(PlotAction::PanRight)
        } else if binding_matches(&code, &bindings.reset_view) {
            Some(PlotAction::Reset)
        } else {
            None
        };
        if let (Some(plot), Some(action)) = (self.plot.as_mut(), action) {
            match action {
                PlotAction::ZoomIn => plot.viewport.zoom_in(),
                PlotAction::ZoomOut => plot.viewport.zoom_out(),
                PlotAction::PanLeft => plot.viewport.pan(InputDirection::Left),
                PlotAction::PanRight => plot.viewport.pan(InputDirection::Right),
                PlotAction::Reset => plot.viewport.reset(),
            }
        }
        Ok(())
    }

    /// Apply vertical input to the selected field.
    fn change_cursor_position(&mut self, direction: InputDirection) {
        match self.flow_state {
            AppState::SelectingConnection(ref selected_menu) => match selected_menu {
                SelectedConnection::Baudrate => self.baudrate.change_cursor(direction),
                SelectedConnection::Port => self.port_list.change_cursor(direction),
            },
            AppState::SelectingScan(ref selected_menu) => match selected_menu {
                SelectedScan::StartFreq => self.start_frequency.change_value(direction),
                SelectedScan::FinalFreq => self.stop_frequency.change_value(direction),
                SelectedScan::Points => self.points.change_value(direction),
                SelectedScan::Repeat => self.repeat.change_value(direction),
            },
            AppState::Complete => (),
            AppState::Scanning => (),
            AppState::Error => (),
        }
    }

    /// Move horizontally within the current screen, clamping at its edges.
    fn next_menu(&mut self, direction: InputDirection) -> Result<(), UiError> {
        if let Some(s) = match self.flow_state {
            AppState::SelectingConnection(ref r) => {
                let v = *r as usize;

                match direction {
                    InputDirection::Right => Some(AppState::SelectingConnection(
                        SelectedConnection::from_repr((v + 1).min(SelectedConnection::COUNT - 1))
                            .ok_or(UiError::MenuSelection)?,
                    )),
                    InputDirection::Left => Some(AppState::SelectingConnection(
                        SelectedConnection::from_repr(v.saturating_sub(1))
                            .ok_or(UiError::MenuSelection)?,
                    )),
                    _ => None,
                }
            }
            AppState::SelectingScan(ref r) => {
                let v = *r as usize;

                match direction {
                    InputDirection::Right => Some(AppState::SelectingScan(
                        SelectedScan::from_repr((v + 1).min(SelectedScan::COUNT - 1))
                            .ok_or(UiError::MenuSelection)?,
                    )),
                    InputDirection::Left => Some(AppState::SelectingScan(
                        SelectedScan::from_repr(v.saturating_sub(1))
                            .ok_or(UiError::MenuSelection)?,
                    )),
                    _ => None,
                }
            }
            AppState::Complete => None,
            AppState::Scanning => None,
            AppState::Error => None,
        } {
            self.flow_state = s;
        }
        self.update_color()?;
        Ok(())
    }

    /// Validate before saving settings or starting the serial worker.
    fn next_step(&mut self) -> Result<(), UiError> {
        match self.flow_state {
            AppState::SelectingConnection(_) => {
                self.port_list.selected_port()?;
                self.baudrate.selected_baudrate()?;
                self.flow_state = AppState::SelectingScan(SelectedScan::default());
            }
            AppState::SelectingScan(_) => {
                self.start_selected_scan()?;
            }
            AppState::Complete => {
                self.should_quit = true;
            }
            AppState::Scanning => {}
            AppState::Error => {
                self.should_quit = true;
            }
        }
        Ok(())
    }

    /// Validate and persist the edited fields, then launch their scan.
    fn start_selected_scan(&mut self) -> Result<(), UiError> {
        let config = self.selected_config()?;
        let request = ScanRequest {
            start_hz: config.start_frequency as u64 * 1_000_000,
            stop_hz: config.stop_frequency as u64 * 1_000_000,
            points: config.points,
            repeat: config.repeat,
        };
        request
            .validate()
            .map_err(tiny_wb::acquisition::AcquisitionError::from)
            .map_err(AppError::from)?;
        config.save(&self.config_path).map_err(AppError::from)?;
        self.launch_scan(request, config)
    }

    /// Relaunch the exact request and settings used by the previous scan.
    fn repeat_last_scan(&mut self) -> Result<(), UiError> {
        let Some((request, config)) = self.last_scan.clone() else {
            return Ok(());
        };
        self.launch_scan(request, config)
    }

    /// Reset progress synchronously and start one acquisition worker.
    fn launch_scan(&mut self, request: ScanRequest, config: Config) -> Result<(), UiError> {
        self.init_progress()?;
        self.scan_state.begin();
        self.flow_state = AppState::Scanning;
        self.plot = None;
        self.last_scan = Some((request, config.clone()));
        let connection = self.scan_state.clone();
        thread::spawn(move || {
            if let Err(error) = scan::run_scan(connection.clone(), request, config) {
                connection.fail(error);
            }
        });
        Ok(())
    }
    /// Transfer completed samples once and keep rendering free of transitions.
    fn refresh_scan(&mut self) -> Result<(), UiError> {
        if matches!(self.flow_state, AppState::Scanning) {
            match self.scan_state.status() {
                ScanStatus::Complete => {
                    let data = self.scan_state.take_samples()?;
                    self.plot = Some(Plot::new(self.scan_state.message(), data));
                    self.flow_state = AppState::Complete;
                }
                ScanStatus::Failed => self.flow_state = AppState::Error,
                ScanStatus::Ready | ScanStatus::Running => {}
            }
        }
        Ok(())
    }
}

/// Renderer action resolved from configured key names.
#[derive(Clone, Copy)]
enum PlotAction {
    /// Decrease the visible span.
    ZoomIn,
    /// Increase the visible span.
    ZoomOut,
    /// Shift toward lower frequencies.
    PanLeft,
    /// Shift toward higher frequencies.
    PanRight,
    /// Restore the full span.
    Reset,
}

/// Match a terminal key against validated character/special-key names.
fn binding_matches(code: &KeyCode, bindings: &[String]) -> bool {
    bindings
        .iter()
        .any(|binding| match (binding.as_str(), code) {
            ("left", KeyCode::Left) | ("right", KeyCode::Right) | ("home", KeyCode::Home) => true,
            (value, KeyCode::Char(character)) => {
                let mut chars = value.chars();
                chars.next() == Some(*character) && chars.next().is_none()
            }
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, crossterm::event::KeyModifiers, Terminal};

    fn app() -> App {
        App::new(
            vec![SerialPortInfo {
                port_name: "test-device".into(),
                port_type: serialport::SerialPortType::Unknown,
            }],
            Config::default(),
            PathBuf::from("unused-settings.toml"),
        )
    }

    #[test]
    fn navigation_preserves_settings_and_ignores_key_releases() {
        let mut app = app();
        app.init_config().unwrap();
        let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        app.handle_start_keys(release).unwrap();
        assert!(!app.should_quit);
        app.next_step().unwrap();
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::StartFreq)
        ));
        app.change_cursor_position(InputDirection::Up);
        assert_eq!(app.start_frequency.value(), 250);
        for _ in 0..10 {
            app.next_menu(InputDirection::Right).unwrap();
        }
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::Repeat)
        ));
        app.config.output.include_header = true;
        assert!(app.selected_config().unwrap().output.include_header);
        app.handle_start_keys(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn frequency_digits_open_an_editor_before_enter_advances() {
        let mut app = app();
        app.init_config().unwrap();
        app.next_step().unwrap();
        for digit in "315".chars() {
            app.handle_start_keys(KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE))
                .unwrap();
        }
        assert_eq!(app.start_frequency.to_string(), "315_");
        app.handle_start_keys(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::StartFreq)
        ));
        app.handle_start_keys(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.start_frequency.value(), 315);
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::StartFreq)
        ));
        app.handle_start_keys(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::FinalFreq)
        ));
        app.handle_start_keys(KeyEvent::new(KeyCode::Char('9'), KeyModifiers::NONE))
            .unwrap();
        app.handle_start_keys(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.stop_frequency.value(), 800);
        assert!(!app.should_quit);
    }

    #[test]
    fn point_digits_use_the_same_bounded_editor() {
        let mut app = app();
        app.init_config().unwrap();
        app.next_step().unwrap();
        app.next_menu(InputDirection::Right).unwrap();
        app.next_menu(InputDirection::Right).unwrap();
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::Points)
        ));

        for digit in "2048".chars() {
            app.handle_start_keys(KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE))
                .unwrap();
        }
        assert_eq!(app.points.to_string(), "2048_");
        app.handle_start_keys(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.points.value(), 2048);
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::Points)
        ));
    }

    #[test]
    fn missing_ports_and_invalid_spans_fail_before_saving_or_scanning() {
        let mut app = App::new(
            Vec::new(),
            Config::default(),
            PathBuf::from("unused-settings.toml"),
        );
        assert!(matches!(app.next_step(), Err(UiError::NoPortSelected)));
        let mut app = self::app();
        app.next_step().unwrap();
        app.start_frequency = NumericInput::new(900, 10, 0, 5_400);
        assert!(app.next_step().is_err());
        assert_eq!(app.scan_state.status(), ScanStatus::Ready);
    }

    #[test]
    fn completion_moves_samples_once_and_failure_selects_error_screen() {
        let mut app = app();
        app.flow_state = AppState::Scanning;
        app.scan_state
            .complete(PathBuf::from("scan.csv"), vec![(240.0, -64.0)]);
        app.refresh_scan().unwrap();
        assert!(matches!(app.flow_state, AppState::Complete));
        assert_eq!(app.plot.as_ref().unwrap().data, vec![(240.0, -64.0)]);
        app.refresh_scan().unwrap();
        app.flow_state = AppState::Scanning;
        app.scan_state.fail("disconnected");
        app.refresh_scan().unwrap();
        assert!(matches!(app.flow_state, AppState::Error));
    }

    #[test]
    fn configured_plot_keys_zoom_pan_and_reset() {
        let mut config = Config::default();
        config.ui.bindings.zoom_in = vec!["z".into()];
        let mut app = App::viewer(
            config,
            PathBuf::from("scan.csv"),
            vec![
                (100.0, -1.0),
                (125.0, -1.5),
                (150.0, -2.0),
                (175.0, -1.5),
                (200.0, -1.0),
            ],
        );
        app.handle_start_keys(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.plot.as_ref().unwrap().viewport.bounds(), [100.0, 200.0]);
        app.handle_start_keys(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.plot.as_ref().unwrap().viewport.bounds(), [125.0, 175.0]);
        app.handle_start_keys(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.plot.as_ref().unwrap().viewport.bounds(), [137.5, 187.5]);
        app.handle_start_keys(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.plot.as_ref().unwrap().viewport.bounds(), [100.0, 200.0]);
    }

    #[test]
    fn new_scan_key_returns_to_prefilled_parameters_only_for_acquisition() {
        let mut app = app();
        app.flow_state = AppState::Complete;
        app.plot = Some(Plot::new("scan.csv", vec![(240.0, -64.0)]));
        app.handle_start_keys(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(
            app.flow_state,
            AppState::SelectingScan(SelectedScan::StartFreq)
        ));
        assert_eq!(app.start_frequency.value(), 240);
        assert!(app.plot.is_none());

        let mut viewer = App::viewer(
            Config::default(),
            PathBuf::from("scan.csv"),
            vec![(240.0, -64.0)],
        );
        viewer
            .handle_start_keys(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(viewer.flow_state, AppState::Complete));
    }

    #[test]
    fn every_screen_renders_without_a_physical_terminal() {
        for (width, height) in [(120, 40), (40, 10), (1, 1)] {
            let mut app = app();
            app.init_config().unwrap();
            app.init_progress().unwrap();
            app.plot = Some(Plot::new("test.csv", vec![(240.0, -64.0)]));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for screen in [
                AppState::default(),
                AppState::SelectingScan(SelectedScan::StartFreq),
                AppState::SelectingScan(SelectedScan::FinalFreq),
                AppState::SelectingScan(SelectedScan::Points),
                AppState::SelectingScan(SelectedScan::Repeat),
                AppState::Scanning,
                AppState::Complete,
                AppState::Error,
            ] {
                app.flow_state = screen;
                terminal.draw(|frame| app.draw(frame).unwrap()).unwrap();
            }
        }
    }
}
