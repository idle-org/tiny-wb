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
    AppState, BaudRateList, InputDirection, MenuItems, NumericInput, PortList, SelectedConnection,
    SelectedScan,
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
                960,
            ),
            stop_frequency: NumericInput::new(
                config.stop_frequency,
                config.scan.frequency_step_mhz,
                10,
                960,
            ),
            points: NumericInput::new(
                config.points,
                config.scan.points_step,
                config.scan.min_points,
                config.scan.max_points,
            ),
            repeat: NumericInput::new(config.repeat, 1, 1, config.scan.max_repeat),
            scan_state: ScanState::default(),
            return_data: Vec::new(),
            config,
            config_path,
        }
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
                self.flow_state = AppState::Scanning;
                self.init_progress()?;
                let connection = self.scan_state.clone();
                thread::spawn(move || {
                    if let Err(error) = scan::run_scan(connection.clone(), request, config) {
                        connection.fail(error);
                    }
                });
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
    /// Transfer completed samples once and keep rendering free of transitions.
    fn refresh_scan(&mut self) -> Result<(), UiError> {
        if matches!(self.flow_state, AppState::Scanning) {
            match self.scan_state.status() {
                ScanStatus::Complete => {
                    self.return_data = self.scan_state.take_samples()?;
                    self.flow_state = AppState::Complete;
                }
                ScanStatus::Failed => self.flow_state = AppState::Error,
                ScanStatus::Ready | ScanStatus::Running => {}
            }
        }
        Ok(())
    }
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
    fn missing_ports_and_invalid_spans_fail_before_saving_or_scanning() {
        let mut app = App::new(
            Vec::new(),
            Config::default(),
            PathBuf::from("unused-settings.toml"),
        );
        assert!(matches!(app.next_step(), Err(UiError::NoPortSelected)));
        let mut app = self::app();
        app.next_step().unwrap();
        app.start_frequency = NumericInput::new(900, 10, 0, 960);
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
        assert_eq!(app.return_data, vec![(240.0, -64.0)]);
        app.refresh_scan().unwrap();
        app.flow_state = AppState::Scanning;
        app.scan_state.fail("disconnected");
        app.refresh_scan().unwrap();
        assert!(matches!(app.flow_state, AppState::Error));
    }

    #[test]
    fn every_screen_renders_without_a_physical_terminal() {
        for (width, height) in [(120, 40), (40, 10), (1, 1)] {
            let mut app = app();
            app.init_config().unwrap();
            app.init_progress().unwrap();
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
