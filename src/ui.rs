//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use clap::builder::Resettable;
use color_eyre::{eyre::Context, owo_colors::OwoColorize, Section};
use core::{panic, time};
use log::info;
use ratatui::{
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        style::Print,
    },
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::BlockExt,
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Styled, Stylize,
    },
    symbols::{self, Marker},
    text::Line,
    widgets::{
        block::Title, Axis, Block, Borders, Chart, Dataset, GraphType, HighlightSpacing, LineGauge,
        List, ListDirection, ListItem, ListState, Padding, Paragraph, ScrollDirection,
        StatefulWidget, Widget, Wrap,
    },
    DefaultTerminal, Frame,
};
use serialport::SerialPortInfo;
use std::{
    borrow::{Borrow, BorrowMut},
    cmp::min,
    fmt::Pointer,
    iter::Iterator,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use strum::{EnumCount, EnumIter, EnumProperty};
use strum::{FromRepr, IntoEnumIterator};
use tui_textarea::{Input, Key, TextArea};

use crate::{
    mleft, mright, scan,
    structs::{
        ConnectionInfo, ConnectionStatus, InputDirection, SavedConfig, TinyError, TinyUIError,
    },
    ui_struct::{
        AppInfo, AppState, BaudRateList, MenuItems, PortList, SelectedConfig, SelectedFinalConfig,
        UIUsize, SELECTED_COLOR, UNSELECTED_COLOR,
    },
};

impl<'a> AppInfo<'a> {
    /// Create a new app with the port info available
    pub(crate) fn new(info: Vec<SerialPortInfo>) -> Self {
        let loaded_conf = SavedConfig::new();
        let (start_frequency, stop_frequency, points, repeat) = {
            if let Ok(c) = &loaded_conf {
                (
                    UIUsize::new(c.start_frequency, 10, 0, 960),
                    UIUsize::new(c.stop_frequency, 10, 10, 960),
                    UIUsize::new(c.points, 100, 101, 1000001),
                    UIUsize::new(c.repeat, 1, 1, 1000),
                )
            } else {
                (
                    UIUsize::new(240, 10, 0, 960),
                    UIUsize::new(960, 10, 10, 960),
                    UIUsize::new(10001, 100, 101, 1000001),
                    UIUsize::new(1, 1, 1, 1000),
                )
            }
        };
        Self {
            should_quit: false,
            baudrate: BaudRateList::from_conf(&loaded_conf),
            port_list: PortList::from_serial(info, &loaded_conf),
            flow_state: AppState::default(),
            status_message: String::new(),
            menu_items: MenuItems::default(),
            start_frequency,
            stop_frequency,
            points,
            repeat,
            connection_info: None,
            return_data: Vec::new(),
            logged_message: false,
        }
    }

    /// Contains the main loop for running the app
    pub(crate) fn run(
        &mut self,
        mut terminal: DefaultTerminal,
    ) -> Result<(), color_eyre::eyre::Error> {
        self.init_config()?;
        loop {
            terminal.draw(|v| self.draw(v).inspect_err(|e| panic!("{}", e)).unwrap())?;
            let timeout = Duration::from_millis(16);
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if let Err(e) = self.handle_start_keys(key) {
                        self.get_connection_info()?.set_error(Some(e));
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

    /// Generic draw loop method
    fn draw(&mut self, frame: &mut Frame<'_>) -> Result<(), TinyUIError> {
        let r = match self.flow_state {
            AppState::InitialConfig(_) => self.draw_config(frame),
            AppState::FinalConfig(_) => self.draw_final_config(frame),
            AppState::Finished => self.draw_progress(frame),
            AppState::EndWindow => self.draw_end_window(frame),
            AppState::Error => self.draw_error(frame),
        };
        if let Err(e) = r {
            self.get_connection_info()?.set_error(Some(e));
            self.flow_state = AppState::Error;
        }
        Ok(())
    }

    /// Initialisation of the config window
    fn init_config(&mut self) -> Result<(), TinyUIError> {
        self.connection_info = Some(ConnectionInfo::new());
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
        self.update_color()
    }

    /// Initialisation of all the ui element for the final configuration elements
    fn init_final_config(&mut self) -> Result<(), TinyUIError> {
        self.menu_items.textboxes = vec![Paragraph::new("")];
        self.update_color()
    }

    /// Initialisation of all the final ui elements
    pub(crate) fn init_end(&self) -> Result<(), TinyUIError> {
        Ok(())
    }

    /// Draw the final configuration labels
    fn draw_final_config(&mut self, frame: &mut Frame<'_>) -> Result<(), TinyUIError> {
        let [title_area, main_area, _, status_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let [_, area_minf, area_maxf, area_points, repeat, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Min(7),
        ])
        .areas(main_area);
        // TODO: Move this in the color update
        let (mif_color, maf_color, poi_color, rpt_color) = match self.flow_state {
            AppState::FinalConfig(SelectedFinalConfig::StartFreq) => (
                SELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::FinalConfig(SelectedFinalConfig::FinalFreq) => (
                UNSELECTED_COLOR,
                SELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::FinalConfig(SelectedFinalConfig::Points) => (
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                SELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::FinalConfig(SelectedFinalConfig::Repeat) => (
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                SELECTED_COLOR,
            ),
            _ => unreachable!(),
        };

        // self.status_message = format!("{:?} ", self.flow_state);
        frame.render_widget(
            Paragraph::new(self.start_frequency.int_to_string())
                .block(Block::bordered().title("Start Frequency"))
                .style(mif_color),
            area_minf,
        );
        frame.render_widget(
            Paragraph::new(self.stop_frequency.int_to_string())
                .block(Block::bordered().title("Stop Frequency"))
                .style(maf_color),
            area_maxf,
        );
        frame.render_widget(
            Paragraph::new(self.points.int_to_string())
                .block(Block::bordered().title("Points"))
                .style(poi_color),
            area_points,
        );
        frame.render_widget(
            Paragraph::new(self.repeat.int_to_string())
                .block(Block::bordered().title("Repeat"))
                .style(rpt_color),
            repeat,
        );

        self.update_color()?;
        frame.render_widget(Paragraph::new(self.status_message.clone()), title_area);
        self.render_footer(frame, status_area);
        Ok(())
    }
    /// Render the selection footer
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(
                "Use ↓↑ to move, ←→ to change status widget and ↲ to validate configuration.",
            )
            .centered(),
            area,
        );
    }

    /// Initialize the progress window
    fn init_progress(&mut self) -> Result<(), TinyUIError> {
        self.menu_items.progress_bar = Some(
            LineGauge::default()
                .filled_style(Style::default().fg(Color::Blue))
                .unfilled_style(Style::default().fg(Color::Red))
                .label("Progress:"),
        );
        Ok(())
    }

    /// Draw the config info
    fn draw_config(&mut self, frame: &mut Frame) -> Result<(), TinyUIError> {
        let [title_area, main_area, status_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let [_, area_ports, area_baud, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            Constraint::Min(7),
        ])
        .areas(main_area);
        let list_ports = self
            .menu_items
            .portlist
            .as_ref()
            .ok_or(TinyUIError::UninitializedItemList("Port", line!(), file!()))?;
        let list_baudrates = self
            .menu_items
            .baudlist
            .as_ref()
            .ok_or(TinyUIError::UninitializedItemList("Baud", line!(), file!()))?;
        // self.status_message = format!("{:?} ", self.flow_state);
        frame.render_stateful_widget(list_ports, area_ports, &mut self.port_list.state);
        frame.render_stateful_widget(list_baudrates, area_baud, &mut self.baudrate.state);
        frame.render_widget(Paragraph::new(self.status_message.clone()), title_area);
        self.render_footer(frame, status_area);
        Ok(())
    }

    /// Draw the progress, updates as the progress is being done
    fn draw_progress(&mut self, frame: &mut Frame<'_>) -> Result<(), TinyUIError> {
        if !self.logged_message {
            info!(
                "First pass of the draw process {:#?}",
                self.get_connection_info()?
            );
            self.logged_message = true;
        }
        let [_, main_s, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(90),
            Constraint::Min(7),
        ])
        .areas(frame.area());
        let [title_area, progress_area, main_area, _, _] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(main_s);
        let (pb, status, message) = {
            let status = self.get_connection_info()?.get_scan_message();
            let status = Paragraph::new(status.clone())
                .block(Block::bordered().title("Operation"))
                .style(SELECTED_COLOR);
            let message = self.get_connection_info()?.get_scan_info();
            let message = Paragraph::new(message.clone())
                .block(Block::bordered().title("TinySA Info"))
                .style(SELECTED_COLOR);
            let pb = self
                .menu_items
                .progress_bar
                .take()
                .ok_or(TinyUIError::UninitializedItemList(
                    "ProgressBar",
                    line!(),
                    file!(),
                ))?
                .ratio(self.get_connection_info()?.get_progress() as f64 / 100.);
            (pb, status, message)
        };
        frame.render_widget(pb.clone(), progress_area);
        frame.render_widget(status, title_area);
        frame.render_widget(message, main_area);
        self.menu_items.progress_bar = Some(pb);
        // TODO: Use the connection status instead
        if self.get_connection_info()?.get_progress() >= 100 {
            self.return_data = self.get_connection_info()?.get_return_data()?;
            self.next_step()?;
        }
        let st = self.get_connection_info()?.get_connection_status();
        #[allow(clippy::single_match)]
        match st {
            // TODO: Add other failures, skip to draw error ?
            ConnectionStatus::ConnectionFailure => self.next_step()?,
            _ => (),
        }
        Ok(())
    }

    /// Get a reference to the connection info if it exists
    fn get_connection_info(&self) -> Result<&ConnectionInfo, TinyUIError> {
        self.connection_info
            .as_ref()
            .ok_or(TinyUIError::NoConnectionInfoCreated)
    }

    /// Draw the last confirmation window
    fn draw_error(&mut self, frame: &mut Frame) -> Result<(), TinyUIError> {
        let [_, main_s, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(90),
            Constraint::Min(7),
        ])
        .areas(frame.area());
        let [_, title_area, _main_area, message_area, _, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(main_s);

        let conn_status = self.get_connection_info()?.get_connection_status();
        let status_message = self.error_message_from_current_error()?;
        let status = Paragraph::new(status_message)
            .block(Block::bordered().title("Status"))
            .style(Color::Red);
        let mut additional = String::new();
        if let Some(m) = &self
            .get_connection_info()?
            .error_raised
            .lock()
            .unwrap()
            .as_ref()
        {
            additional.push_str(&format!("Error: {:?} \n => {}\n", m, m));
        }
        let error = Paragraph::new(format!(
            "Last status: {:?}\nLast message: {}\n{}",
            conn_status,
            self.get_connection_info()?.get_scan_message(),
            additional
        ))
        .block(Block::bordered().title("Status"));
        let footer = Paragraph::new("Press ↲ or q to exit".to_string()).centered();
        frame.render_widget(status, title_area);
        frame.render_widget(error, message_area);
        frame.render_widget(footer, footer_area);
        Ok(())
    }

    /// Extract a user explaination for an error message
    fn error_message_from_current_error(&self) -> Result<String, TinyUIError> {
        let status_message = match self
            .get_connection_info()?
            .error_raised
            .lock()
            .unwrap()
            .as_ref()
            .unwrap_or(&TinyUIError::NoErrorSpecified)
        {
            TinyUIError::MenuSelectionError => "An error occured while trying to select a menu item",
            TinyUIError::UninitializedItemList(i, _l, _ff) => &format!("The item {i} was left uninitialized"),
            TinyUIError::KeyPress(_) => "An error occured while processing key events",
            TinyUIError::NoPortSelected => "No connection port was selected",
            TinyUIError::NoConnectionInfoCreated => {
                "An error occured when tring to acquire connection info with the TinySA"
            }
            TinyUIError::NoReturnData => "An error occured when trying to acquire the return data with the TinySA",
            TinyUIError::NoErrorSpecified => "An unspecified error occured",
            TinyUIError::ScanError(TinyError::ParseBeggining(_)) => {
                "An error occured while receiving data, the scan never sent back any valid data"
            }
            TinyUIError::ScanError(TinyError::ParseEnd(_)) => {
                "An error occured while receiving data, scan was interupted before ending properly"
            }
            TinyUIError::ScanError(TinyError::PortConnection(_)) => {
                "An error occured while trying to connect, check the selected port and baudrate"
            }
            TinyUIError::ScanError(TinyError::NoVersionFound) => {
                "An error forbid the gathering of the version, check your TinySA is connected and powered on"
            }
            TinyUIError::ScanError(TinyError::BadScanFrequencies(_)) => {
                "An error occured because the scan frequencies weren't set correctly"
            }
            TinyUIError::ScanError(TinyError::OpenOutput(_)) => "An error occured while trying to export the scan",
            TinyUIError::ScanError(TinyError::TUIInitialization(_)) => {
                "An error occured while trying to create the TUI"
            }
            TinyUIError::ScanError(TinyError::TUIRuntimeError(_)) => "An error occured during runtime",
            TinyUIError::ScanError(TinyError::TUIPackingError(_)) => {
                "An error occured while trying to parse the data points"
            }
            TinyUIError::ScanError(TinyError::ConfigNotFound) => {
                "An error occured while trying to open the config file"
            }
            TinyUIError::ScanError(TinyError::InvalidConfig(_)) => {
                "An error forbid to read the config as it seems invalid, please delete config.toml and retry"
            }
            TinyUIError::ScanError(TinyError::InvalidDataForConfig(_)) => "An error forbit to write the config",
            TinyUIError::ScanError(TinyError::LoggerError(_))=> "Couldn't initialize the error logger",
            TinyUIError::ScanError(TinyError::SendError(_))=> "Couldn't finish sending data to TinySA",
            TinyUIError::SerializeError(_) => "An error occured while tring to save the current config", 
        };
        Ok(status_message.to_owned())
    }

    /// Draw the last confirmation window
    fn draw_end_window(&mut self, frame: &mut Frame) -> Result<(), TinyUIError> {
        let [_, main_s, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(90),
            Constraint::Min(7),
        ])
        .areas(frame.area());
        let [title_area, _main_area, graph_area, _, footer_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(main_s);

        let status = self.get_connection_info()?.get_connection_status();
        match status {
            ConnectionStatus::ConnectionFailure => {
                self.flow_state = AppState::Error;
            }
            _ => {
                let status = Paragraph::new(format!(
                    "The scan of {} points in the range ({}Mhz - {}Mhz) finished sucessfuly",
                    self.points.int_to_usize(),
                    self.start_frequency.int_to_usize(),
                    self.stop_frequency.int_to_usize(),
                ))
                .block(Block::bordered().title("Status"))
                .style(SELECTED_COLOR);

                let datasets = vec![Dataset::default()
                    .marker(Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::new().yellow())
                    .data(&self.return_data)];

                let chart =
                    Chart::new(datasets)
                        .x_axis(
                            Axis::default()
                                .bounds([
                                    self.start_frequency.int_to_usize() as f64,
                                    self.stop_frequency.int_to_usize() as f64,
                                ])
                                .style(Style::default().fg(Color::Gray))
                                .labels([
                                    format!("{}MHz", self.start_frequency.int_to_string()),
                                    format!(
                                        "{}MHz",
                                        self.start_frequency.int_to_usize()
                                            + (self.stop_frequency.int_to_usize().saturating_sub(
                                                self.start_frequency.int_to_usize()
                                            )) / 2
                                    ),
                                    format!("{}MHz", self.stop_frequency.int_to_string()),
                                ]),
                        )
                        .y_axis(
                            Axis::default()
                                .bounds([-128., 0.])
                                .style(Style::default().fg(Color::Gray))
                                .labels(["-128dbm", "-64dbm", "0dbm"]),
                        );
                frame.render_widget(chart, graph_area);
                let footer = Paragraph::new(format!(
                    "{}, press ↲ or q to exit",
                    self.get_connection_info()?.get_scan_message(),
                ))
                .centered();
                frame.render_widget(status, title_area);
                frame.render_widget(footer, footer_area);
            }
        }
        Ok(())
    }

    /// Handle keys while in the first menu
    fn handle_start_keys(&mut self, key: KeyEvent) -> Result<(), TinyUIError> {
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
                AppState::Finished => (),
                _ => self.next_step()?,
            },
            _ => {}
        }
        Ok(())
    }

    /// A directionnal input was received
    fn change_cursor_position(&mut self, direction: InputDirection) {
        match self.flow_state {
            AppState::InitialConfig(ref selected_menu) => match selected_menu {
                SelectedConfig::Baudrate => self.baudrate.change_cursor(direction),
                SelectedConfig::Port => self.port_list.change_cursor(direction),
            },
            AppState::FinalConfig(ref selected_menu) => match selected_menu {
                SelectedFinalConfig::StartFreq => self.start_frequency.change_value(direction),
                SelectedFinalConfig::FinalFreq => self.stop_frequency.change_value(direction),
                SelectedFinalConfig::Points => self.points.change_value(direction),
                SelectedFinalConfig::Repeat => self.repeat.change_value(direction),
            },
            AppState::EndWindow => (),
            AppState::Finished => (),
            AppState::Error => (),
        }
    }

    /// Goes to the next menu depending on the position in the program
    fn next_menu(&mut self, direction: InputDirection) -> Result<(), TinyUIError> {
        if let Some(s) = match self.flow_state {
            AppState::InitialConfig(ref r) => {
                let v = *r as usize;

                match direction {
                    InputDirection::Right => mright!(v, InitialConfig, SelectedConfig),
                    InputDirection::Left => mleft!(v, InitialConfig, SelectedConfig),
                    _ => None,
                }
            }
            AppState::FinalConfig(ref r) => {
                let v = *r as usize;

                match direction {
                    InputDirection::Right => mright!(v, FinalConfig, SelectedFinalConfig),
                    InputDirection::Left => mleft!(v, FinalConfig, SelectedFinalConfig),
                    _ => None,
                }
            }
            AppState::EndWindow => None,
            AppState::Finished => None,
            AppState::Error => None,
        } {
            self.flow_state = s;
        }
        self.update_color()?;
        Ok(())
    }

    /// Progress in the flow of the program
    fn next_step(&mut self) -> Result<(), TinyUIError> {
        match self.flow_state {
            AppState::InitialConfig(_) => {
                self.flow_state = AppState::FinalConfig(SelectedFinalConfig::default());
                self.init_final_config()?;
            }
            AppState::FinalConfig(_) => {
                self.flow_state = AppState::Finished;
                self.init_progress()?;
                let mut co = self.connection_info.take();
                co.as_mut()
                    .ok_or(TinyUIError::NoConnectionInfoCreated)?
                    .fill(
                        self.port_list.selected_port()?,
                        self.baudrate.selected_baudrate()?,
                        self.start_frequency.mhz_to_usize(),
                        self.stop_frequency.mhz_to_usize(),
                        self.points.int_to_usize(),
                        self.repeat.int_to_usize(),
                    );
                self.connection_info = co.clone();
                let co = co.unwrap();
                let connection = self.get_connection_info()?.clone();
                self.save_config()?;
                thread::spawn(move || {
                    let status = scan::scan_with_progress(co);
                    match status {
                        Ok(_) => {
                            connection.update_connection_status(ConnectionStatus::ClosedWithSuccess)
                        }
                        Err(e) => {
                            info!("An error occured during thread exec: {e}");
                            if !connection.has_error() {
                                connection.set_error(Some(e.into()));
                            }
                            connection.update_connection_status(ConnectionStatus::ConnectionFailure)
                        }
                    }
                });
            }
            AppState::EndWindow => {
                self.should_quit = true;
            }
            AppState::Finished => {
                self.flow_state = AppState::EndWindow;
                self.init_end()?;
            }
            AppState::Error => {
                self.should_quit = true;
            }
        }
        Ok(())
    }
}
