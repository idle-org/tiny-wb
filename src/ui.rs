//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use clap::builder::Resettable;
use color_eyre::{owo_colors::OwoColorize, Section};
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
    structs::{InputDirection, TinyError, TinyUIError},
    ui_struct::{
        AppInfo, AppState, BaudRateList, MenuItems, PortList, SelectedConfig, SelectedFinalConfig,
        UIUsize, SELECTED_COLOR, UNSELECTED_COLOR,
    },
};

impl<'a> AppInfo<'a> {
    /// Create a new app with the port info available
    pub(crate) fn new(info: Vec<SerialPortInfo>) -> Self {
        Self {
            should_quit: false,
            baudrate: BaudRateList::default(),
            port_list: PortList::from_serial(info),
            flow_state: AppState::default(),
            status_message: String::new(),
            menu_items: MenuItems::default(),
            start_frequency: UIUsize::new(240, 10, 960),
            stop_frequency: UIUsize::new(960, 10, 960),
            points: UIUsize::new(10000, 100, 1000000),
            progress: Arc::new(Mutex::new(0)),
            scan_info: Arc::new(Mutex::new(String::new())),
            scan_message: Arc::new(Mutex::new(String::new())),
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
            // _ => unimplemented!(),
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
        let [title_area, main_area, graph_area, _, footer_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(main_s);
        let status = Paragraph::new(format!(
            "The scan ({}Mhz - {}Mhz) of {} points finished sucessfuly",
            self.start_frequency.int_to_usize(),
            self.stop_frequency.int_to_usize(),
            self.points.int_to_usize()
        ))
        .block(Block::bordered().title("Status"))
        .style(SELECTED_COLOR);
        // let dataset = Dataset::default()
        //     .marker(symbols::Marker::HalfBlock)
        //     .style(Style::new().fg(Color::Blue))
        //     .graph_type(GraphType::Bar)
        //     // a bell curve
        //     .data(&[
        //         (250., -2.9),
        //         (260., -13.5),
        //         (270., -41.1),
        //         (280., -80.1),
        //         (390., -100.0),
        //         (500., -80.1),
        //         (610., -41.1),
        //         (720., -13.5),
        //         (830., -2.9),
        //         (930., -2.9),
        //     ]);

        // let chart = Chart::new(vec![dataset])
        //     .block(
        //         Block::bordered().title(
        //             Title::default()
        //                 .content("Bar chart".cyan().bold())
        //                 .alignment(Alignment::Center),
        //         ),
        //     )
        //     .x_axis(
        //         Axis::default()
        //             .style(Style::default().gray())
        //             .bounds([
        //                 self.start_frequency.mhz_to_usize() as f64,
        //                 self.stop_frequency.mhz_to_usize() as f64,
        //             ])
        //             .labels([
        //                 self.start_frequency.int_to_string().bold(),
        //                 format!(
        //                     "{}",
        //                     (self.stop_frequency.int_to_usize()
        //                         - self.start_frequency.int_to_usize() / 2)
        //                 )
        //                 .into(),
        //                 self.stop_frequency.int_to_string().bold(),
        //             ]),
        //     )
        //     .y_axis(
        //         Axis::default()
        //             .style(Style::default().gray())
        //             .bounds([-128.0, 0.])
        //             .labels(["-128".bold(), "-64".into(), "0".bold()]),
        //     );
        // // .hidden_legend_constraints((Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)));

        let datasets = vec![Dataset::default()
            .name("Heavy")
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::new().yellow())
            .data(&[
                (1965., -82.),
                (1967., -54.),
                (1981., -65.),
                (1989., -30.),
                (1997., -10.),
                (2004., -11.),
                (2014., -45.),
                (2016., -79.),
                (2018., -15.),
            ])];

        let chart = Chart::new(datasets)
            .block(
                Block::bordered().title(
                    Title::default()
                        .content("Scatter chart".cyan().bold())
                        .alignment(Alignment::Center),
                ),
            )
            .x_axis(
                Axis::default()
                    .title("Year")
                    .bounds([1960., 2020.])
                    .style(Style::default().fg(Color::Gray))
                    .labels(["1960", "1990", "2020"]),
            )
            .y_axis(
                Axis::default()
                    .title("Cost")
                    .bounds([-128., 0.])
                    .style(Style::default().fg(Color::Gray))
                    .labels(["0", "37 500", "75 000"]),
            )
            .hidden_legend_constraints((Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)));
        frame.render_widget(chart, graph_area);
        let footer =
            Paragraph::new("Press ↲ to save the data as a csv or q to exit without saving.")
                .centered();
        frame.render_widget(status, title_area);
        frame.render_widget(footer, footer_area);
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
                let pmtx = self.progress.clone();
                let mmtx = self.scan_info.clone();
                let cmtx = self.scan_message.clone();
                let port = self.port_list.selected_port();
                let start = self.start_frequency.mhz_to_usize();
                let stop = self.stop_frequency.mhz_to_usize();
                let points = self.points.int_to_usize();
                // BYPASS
                *self.progress.lock().unwrap() = 100;
                // thread::spawn(move || {
                //     scan::scan_with_progress(pmtx, mmtx, cmtx, port, start, stop, points).unwrap()
                // });
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
