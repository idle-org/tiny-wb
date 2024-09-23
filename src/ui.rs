//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use color_eyre::{owo_colors::OwoColorize, Result};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::BlockExt,
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Styled, Stylize,
    },
    symbols,
    text::Line,
    widgets::{
        Block, Borders, HighlightSpacing, List, ListDirection, ListItem, ListState, Padding,
        Paragraph, ScrollDirection, StatefulWidget, Widget, Wrap,
    },
    DefaultTerminal, Frame,
};
use serialport::SerialPortInfo;
use std::{
    borrow::{Borrow, BorrowMut},
    cmp::min,
    fmt::Pointer,
    iter::Iterator,
    time::Instant,
};
use strum::{EnumCount, EnumIter, EnumProperty};
use strum::{FromRepr, IntoEnumIterator};
use tui_textarea::{Input, Key, TextArea};

use crate::structs::{InputDirection, TinyError, TinyUIError};

/// The header style
const HEADER_STYLE: Style = Style::new().fg(SLATE.c100).bg(BLUE.c800);
/// The background color
const NORMAL_ROW_BG: Color = SLATE.c950;
/// Background color for alternated rows
const ALT_ROW_BG_COLOR: Color = SLATE.c900;
/// The style applied to the currently selected item
const SELECTED_STYLE: Style = Style::new().bg(SLATE.c800).add_modifier(Modifier::BOLD);
/// Foreground color
const TEXT_FG_COLOR: Color = SLATE.c200;
/// Color of a selected border
const SELECTED_COLOR: Color = Color::White;
/// Color of an unselected item
const UNSELECTED_COLOR: Color = Color::DarkGray;

/// This struct holds the current state of the app.
#[derive(Debug)]
pub(crate) struct AppInfo<'a> {
    /// Wether the app sould immediately quit
    should_quit: bool,
    /// The list of baudrates
    baudrate: BaudRateList,
    /// The list of ports
    port_list: PortList,
    /// State of the configuration, akin to tab or window
    flow_state: AppState,
    /// The list of menu item generated
    menu_items: MenuItems<'a>,
    /// A debug message string
    status_message: String,
}

// #[derive(ItemSelectedState)]
// /// The item currently selected

#[derive(Debug)]
/// The inner state of the app, useful for display and keystrokes
enum AppState {
    /// Initial config, baudrate and port
    InitialConfig(SelectedConfig),
    /// Final config, frequencies and points
    FinalConfig(SelectedFinalConfig),
    /// Popup for text entering
    Popup,
    /// Save window
    Finished,
}

impl Default for AppState {
    fn default() -> Self {
        Self::InitialConfig(SelectedConfig::default())
    }
}

/// Shortcut for move right
macro_rules! mright {
    ($v:ident, $cname:ident,$fname:ident) => {
        Some(AppState::$cname(
            $fname::from_repr(min($v + 1, $fname::COUNT - 1))
                .ok_or(TinyUIError::MenuSelectionError)?,
        ))
    };
}

/// Shortcut to move left
macro_rules! mleft {
    ($v:ident, $cname:ident,$fname:ident) => {
        if $v == 0 {
            None
        } else {
            Some(AppState::$cname(
                $fname::from_repr($v - 1).ok_or(TinyUIError::MenuSelectionError)?,
            ))
        }
    };
}

impl<'a> AppInfo<'a> {
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
            AppState::Popup => None,
            AppState::Finished => None,
        } {
            self.flow_state = s;
        }
        self.update_color()?;
        Ok(())
    }

    /// Put an item a specified color
    fn change_color(&mut self, name: &'a str, list: List<'a>, color: Color) -> Option<List<'a>> {
        Some(
            list.block(Block::bordered().title(name).border_style(color))
                .style(Style::default().fg(color)),
        )
    }

    /// Progress in the flow of the program
    fn next_step(&mut self) {
        match self.flow_state {
            AppState::InitialConfig(_) => {
                self.flow_state = AppState::FinalConfig(SelectedFinalConfig::default())
            }
            AppState::FinalConfig(_) => self.flow_state = AppState::Finished,
            AppState::Popup => todo!(),
            AppState::Finished => todo!(),
        }
    }

    /// Update all the colors
    fn update_color(&mut self) -> Result<(), TinyUIError> {
        match self.flow_state {
            AppState::InitialConfig(ref c) => {
                let (p_c, b_c) = match c {
                    SelectedConfig::Baudrate => (UNSELECTED_COLOR, SELECTED_COLOR),
                    SelectedConfig::Port => (SELECTED_COLOR, UNSELECTED_COLOR),
                };
                let (p, b) =
                    (
                        self.menu_items.portlist.take().ok_or(
                            TinyUIError::UninitializedItemList("Port List", line!(), file!()),
                        )?,
                        self.menu_items.baudlist.take().ok_or(
                            TinyUIError::UninitializedItemList("Port List", line!(), file!()),
                        )?,
                    );
                // self.status_message = format!("{:?} p:{:?} {:?} {:?}", Instant::now(), p_c, b_c, c);
                self.menu_items.portlist = self.change_color("COM Ports", p, p_c);
                self.menu_items.baudlist = self.change_color("Baudrate", b, b_c);
            }
            AppState::FinalConfig(ref c) => {
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

                // let (mif_menu, maf_menu, poi_menu) = (
                //     self.menu_items.min_frequency.take()?,
                //     self.menu_items.max_frequency.take()?,
                //     self.menu_items.points.take()?,
                // );
                // self.change_color(mif_menu, mif_color);
                // self.change_color(maf_menu, maf_color);
                // self.change_color(poi_menu, poi_color);
            }
            AppState::Popup => todo!(),
            AppState::Finished => todo!(),
        }
        Ok(())
    }
}

/// The selected item in the config
#[derive(Debug, EnumCount, FromRepr, Clone, Copy)]
enum SelectedConfig {
    /// Selects the ports
    Port,
    /// Selects the baudrate
    Baudrate,
    // /// Selects <Ok> to proceed
    // Proceed,
}

impl Default for SelectedConfig {
    fn default() -> Self {
        Self::Port
    }
}

/// The selected item in the final menu
#[derive(Debug, EnumCount, FromRepr, Copy, Clone)]
enum SelectedFinalConfig {
    /// Selects the start frequency
    StartFreq,
    /// Selects the final frequency
    FinalFreq,
    /// Selects the number of points
    Points,
    // /// Selects <Ok> to proceed
    // Proceed,
}

impl Default for SelectedFinalConfig {
    fn default() -> Self {
        Self::StartFreq
    }
}

/// The list of all baudrates selectables
#[derive(Debug)]
struct BaudRateList {
    /// The selected baudrate (default : BAUD1115200)
    selected: usize,
    /// The baudrate list
    baudrates: Vec<BaudRate>,
    /// The inner state of the widget
    state: ListState,
}
impl BaudRateList {
    /// Convert into a vec of strings, this is extremely expensive and shouldn't be done that way
    fn to_vecstr(&self) -> Vec<String> {
        self.baudrates
            .iter()
            .map(|i| format!("{}", *i as usize))
            .collect()
    }

    /// Change the cursor position on the baudrates
    fn change_cursor(&mut self, direction: InputDirection) {
        match direction {
            InputDirection::Down => self.state.select_next(),
            InputDirection::Up => self.state.select_previous(),
            _ => (),
        }
    }
}

impl Default for BaudRateList {
    fn default() -> Self {
        Self {
            selected: 0,
            baudrates: BaudRate::iter().collect::<Vec<_>>(),
            state: ListState::default().with_selected(Some(3)),
        }
    }
}

/// A list of possible baudrates
#[derive(Debug, EnumIter, Copy, Clone)]
enum BaudRate {
    BAUD19200 = 19_200,
    BAUD38400 = 38_400,
    BAUD57600 = 57_600,
    BAUD115200 = 115_200,
    BAUD230400 = 230_400,
    BAUD460800 = 460_800,
    BAUD921600 = 921_600,
    BAUD1843200 = 1_843_200,
    BAUD2000000 = 2_000_000,
    BAUD3000000 = 3_000_000,
}

/// A wrapper over the list of ports
#[derive(Debug)]
pub(crate) struct PortList {
    /// The list of found ports
    ports: Vec<String>,
    /// The currently selected port (doing it that way completly avoids the )
    selected: usize,
    /// The inner state of the widget
    state: ListState,
}

impl PortList {
    /// Make a port list from a serial reading
    pub(crate) fn from_serial(info: Vec<SerialPortInfo>) -> Self {
        let ports: Vec<String> = info.into_iter().map(|port| port.port_name).collect();
        let selected = find_best_port(&ports);
        Self {
            ports,
            selected,
            state: ListState::default().with_selected(Some(selected)),
        }
    }

    /// Convert the list of ports
    fn to_vecstr(&self) -> Vec<String> {
        self.ports.clone()
    }

    /// Change the selected item
    fn change_cursor(&mut self, direction: InputDirection) {
        match direction {
            InputDirection::Up => self.state.select_previous(),
            InputDirection::Down => self.state.select_next(),
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
/// The static list of all menu items
struct MenuItems<'a> {
    /// List of port items
    portlist: Option<List<'a>>,
    /// List of port items
    baudlist: Option<List<'a>>,
}

// impl<'a> Default for MenuItems<'a> {
//     fn default() -> Self {
//         Self { portlist: None }
//     }
// }
/// Try to guess the best port to preselect
fn find_best_port(ports: &[String]) -> usize {
    let best_ports = ["/dev/ttyACM0", "COM3", "COM5", "/dev/ttyS16"];
    for port in best_ports {
        if let Some(p) = ports.iter().position(|v| v == port) {
            return p;
        }
    }
    0
}

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
        }
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
            // KeyCode::Char('g') | KeyCode::Home => self.select_first(),
            // KeyCode::Char('G') | KeyCode::End => self.select_last(),
            KeyCode::Enter => {
                self.next_step();
            }
            _ => {}
        }
        Ok(())
    }

    /// Contains the main loop for running the app
    pub(crate) fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.init_config();
        loop {
            terminal.draw(|v| self.draw(v).inspect_err(|e| panic!("{}", e)).unwrap())?;
            if let Event::Key(key) = event::read()? {
                self.handle_start_keys(key)?;
            };
            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    /// Generic draw loop method
    fn draw(&mut self, frame: &mut Frame<'_>) -> Result<(), TinyUIError> {
        match self.flow_state {
            AppState::InitialConfig(_) => self.draw_config(frame),
            AppState::FinalConfig(_) => self.draw_final_config(frame),
            // AppState::Popup => todo!(),
            // AppState::Finished => todo!(),
            _ => todo!(),
        }
    }
    /// Initialisation of the config window
    fn init_config(&mut self) {
        // self.menu_items.baudlist.new()
        let (b_color, p_color) = match self.flow_state {
            AppState::InitialConfig(SelectedConfig::Port) => (UNSELECTED_COLOR, SELECTED_COLOR),
            AppState::InitialConfig(SelectedConfig::Baudrate) => (SELECTED_COLOR, UNSELECTED_COLOR),
            _ => unreachable!(),
        };
        self.menu_items.portlist = Some(
            List::new(self.port_list.to_vecstr())
                .block(Block::bordered().title("COM Ports").border_style(p_color))
                .style(Style::default().fg(p_color))
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::ITALIC)
                        .bg(Color::Blue),
                )
                .repeat_highlight_symbol(true),
        );
        self.menu_items.baudlist = Some(
            List::new(self.baudrate.to_vecstr())
                .block(Block::bordered().title("Baudrates").border_style(b_color))
                .style(Style::default().fg(b_color))
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::ITALIC)
                        .bg(Color::Blue),
                )
                .repeat_highlight_symbol(true),
        );
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

        self.status_message = format!("{:?} ", self.flow_state);
        frame.render_widget(
            Paragraph::new("Fin")
                .block(Block::bordered())
                .style(mif_color),
            area_minf,
        );
        frame.render_widget(
            Paragraph::new("Fin")
                .block(Block::bordered())
                .style(maf_color),
            area_maxf,
        );
        frame.render_widget(
            Paragraph::new("Fin")
                .block(Block::bordered())
                .style(poi_color),
            area_points,
        );

        // frame.render_stateful_widget(list_baudrates, area_baud, &mut self.baudrate.state);
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

    /// Draw the popup for frequency input
    fn draw_freq_popup(&self) {
        // self.popup = SelectedFinalConfig::StartFreq;
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightBlue))
                .title("Crossterm Popup Example"),
        );

        let area = Rect {
            width: 40,
            height: 5,
            x: 5,
            y: 5,
        };
        textarea.set_style(Style::default().fg(Color::Yellow));
        textarea.set_placeholder_style(Style::default());
        textarea.set_placeholder_text("prompt message");
    }

    /// A directionnal input was received
    fn change_cursor_position(&mut self, direction: InputDirection) {
        match self.flow_state {
            AppState::InitialConfig(ref selected_menu) => match selected_menu {
                SelectedConfig::Baudrate => self.baudrate.change_cursor(direction),
                SelectedConfig::Port => self.port_list.change_cursor(direction),
            },
            AppState::FinalConfig(ref selected_menu) => match selected_menu {
                SelectedFinalConfig::StartFreq => self.draw_freq_popup(),
                SelectedFinalConfig::FinalFreq => todo!(),
                SelectedFinalConfig::Points => todo!(),
            },
            AppState::Popup => (),
            AppState::Finished => (),
        }
    }
}

// #[allow(clippy::missing_docs_in_private_items)]
// const fn alternate_colors(i: usize) -> Color {
//     if i % 2 == 0 {
//         NORMAL_ROW_BG
//     } else {
//         ALT_ROW_BG_COLOR
//     }
// }
