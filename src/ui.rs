//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use clap::builder::Resettable;
use color_eyre::{eyre::Context, owo_colors::OwoColorize, Section};
use core::{panic, time};
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
    structs::{ConnectionStatus, InputDirection, SavedConfig, TinyError, TinyUIError},
    ui_struct::{
        AppInfo, AppState, BaudRateList, MenuItems, PortList, SelectedConfig, SelectedFinalConfig,
        UIUsize, SELECTED_COLOR, UNSELECTED_COLOR,
    },
};

impl<'a> AppInfo<'a> {
    /// Create a new app with the port info available
    pub(crate) fn new(info: Vec<SerialPortInfo>) -> Self {
        let loaded_conf = SavedConfig::new();
        let (start_frequency, stop_frequency, points) = {
            if let Ok(c) = &loaded_conf {
                (
                    UIUsize::new(c.start_frequency, 10, 960),
                    UIUsize::new(c.stop_frequency, 10, 960),
                    UIUsize::new(c.points, 100, 1000000),
                )
            } else {
                (
                    UIUsize::new(240, 10, 960),
                    UIUsize::new(960, 10, 960),
                    UIUsize::new(10000, 100, 1000000),
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
            progress: Arc::new(Mutex::new(0)),
            scan_info: Arc::new(Mutex::new(String::new())),
            scan_message: Arc::new(Mutex::new(String::new())),
            return_data: Arc::new(Mutex::new(Vec::new())),
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Uninitialised)),
            repeat: UIUsize::new(1, 1, 1000),
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
                    self.handle_start_keys(key)?;
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
        match self.flow_state {
            AppState::InitialConfig(_) => self.draw_config(frame),
            AppState::FinalConfig(_) => self.draw_final_config(frame),
            AppState::Finished => self.draw_progress(frame),
            AppState::EndWindow => self.draw_end_window(frame),
        }
    }

    /// Initialisation of the config window
    fn init_config(&mut self) -> Result<(), TinyUIError> {
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
        let [_, area_minf, area_maxf, area_points, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Min(7),
        ])
        .areas(main_area);
        // TODO: Move this in the color update
        let (mif_color, maf_color, poi_color) = match self.flow_state {
            AppState::FinalConfig(SelectedFinalConfig::StartFreq) => {
                (SELECTED_COLOR, UNSELECTED_COLOR, UNSELECTED_COLOR)
            }
            AppState::FinalConfig(SelectedFinalConfig::FinalFreq) => {
                (UNSELECTED_COLOR, SELECTED_COLOR, UNSELECTED_COLOR)
            }
            AppState::FinalConfig(SelectedFinalConfig::Points) => {
                (UNSELECTED_COLOR, UNSELECTED_COLOR, SELECTED_COLOR)
            }
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
                // .block(title)
                .filled_style(Style::default().fg(Color::Blue))
                .unfilled_style(Style::default().fg(Color::Red))
                //             .filled_style(Style::default().fg(Color::Blue).bg(Color::Blue))
                // .unfilled_style(Style::default().fg(Color::Red).bg(Color::Red))
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
            let status = self.scan_message.try_lock().unwrap();
            let status = Paragraph::new(status.clone())
                .block(Block::bordered().title("Operation"))
                .style(SELECTED_COLOR);
            let message = self.scan_info.try_lock().unwrap();
            let message = Paragraph::new(message.clone())
                .block(Block::bordered().title("TinySA Info"))
                .style(SELECTED_COLOR);
            let progress = self.progress.try_lock().unwrap();

            let pb = self
                .menu_items
                .progress_bar
                .take()
                .ok_or(TinyUIError::UninitializedItemList(
                    "ProgressBar",
                    line!(),
                    file!(),
                ))?
                .ratio(*progress as f64 / 100.);
            (pb, status, message)
        };
        frame.render_widget(pb.clone(), progress_area);
        frame.render_widget(status, title_area);
        frame.render_widget(message, main_area);
        self.menu_items.progress_bar = Some(pb);
        if *self.progress.lock().unwrap() >= 100 {
            self.next_step()?;
        }
        let st = *self.connection_status.lock().unwrap();
        #[allow(clippy::single_match)]
        match st {
            // TODO: Add other failures
            ConnectionStatus::ConnectionFailure => self.next_step()?,
            _ => (),
        }
        Ok(())
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

        let status = *self.connection_status.lock().unwrap();
        match status {
            // TODO: | ConnectionStatus::ClosedWithError
            ConnectionStatus::ConnectionFailure => {
                let status = Paragraph::new(
                    // TODO: Be more explicit with errors
                    "The scan couldn't be performed, please check your connection".to_string(),
                )
                .block(Block::bordered().title("Status"))
                .style(Color::Red);
                let footer = Paragraph::new(format!(
                    "{}, press ↲ or q to exit",
                    self.scan_message.lock().unwrap().clone()
                ))
                .centered();
                frame.render_widget(status, title_area);
                frame.render_widget(footer, footer_area);
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

                // TODO: Don't acquire a mutex at 60Hz
                let binding = self.return_data.lock().unwrap();
                let datasets = vec![Dataset::default()
                    // .name("Heavy")
                    .marker(Marker::Dot)
                    .graph_type(GraphType::Scatter)
                    .style(Style::new().yellow())
                    .data(&binding)];

                let chart = Chart::new(datasets)
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
                                        + (self.stop_frequency.int_to_usize()
                                            - self.start_frequency.int_to_usize())
                                            / 2
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
                    self.scan_message.lock().unwrap().clone()
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
            },
            AppState::EndWindow => (),
            AppState::Finished => (),
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
                // Start the scan
                // TODO: Have a struct to avoid this clusterfk
                let pmtx = self.progress.clone();
                let mmtx = self.scan_info.clone();
                let cmtx = self.scan_message.clone();
                let rdata = self.return_data.clone();
                let cstatus = self.connection_status.clone();
                let port = self.port_list.selected_port();
                let start = self.start_frequency.mhz_to_usize();
                let stop = self.stop_frequency.mhz_to_usize();
                let points = self.points.int_to_usize();
                let repeat = self.repeat.int_to_usize();
                // BYPASS
                *self.progress.lock().unwrap() = 100;
                self.save_config()?;
                thread::spawn(move || {
                    let status = scan::scan_with_progress(
                        pmtx, mmtx, cmtx, rdata, port, start, stop, points, repeat,
                    );
                    match status {
                        Ok(_) => *cstatus.lock().unwrap() = ConnectionStatus::ClosedWithSuccess,
                        Err(_) => *cstatus.lock().unwrap() = ConnectionStatus::ConnectionFailure,
                    }
                    // panic!();
                    // TODO: Do something with bad status to avoid false positive
                });
                // scan::scan_with_progress(pmtx, mmtx, cmtx, port).unwrap();
            }
            AppState::EndWindow => {
                self.should_quit = true;
            }
            AppState::Finished => {
                self.flow_state = AppState::EndWindow;
                self.init_end()?;
            }
        }
        Ok(())
    }
}
