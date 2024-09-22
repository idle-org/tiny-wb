//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use color_eyre::{owo_colors::OwoColorize, Result};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Stylize,
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
use std::{cmp::min, fmt::Pointer, iter::Iterator};
use strum::{EnumCount, EnumIter, EnumProperty};
use strum::{FromRepr, IntoEnumIterator};

use crate::structs::InputDirection;

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
pub(crate) struct AppInfo {
    /// Wether the app sould immediately quit
    should_quit: bool,
    /// The list of baudrates
    baudrate: BaudRateList,
    /// The list of ports
    port_list: PortList,
    /// State of the configuration, akin to tab or window
    flow_state: AppState,
    // /// Selected item
    // item_selected: ItemSelectedState
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
            $fname::from_repr(min($v + 1, $fname::COUNT - 1)).unwrap(),
        ))
    };
}

/// Shortcut to move left
macro_rules! mleft {
    ($v:ident, $cname:ident,$fname:ident) => {
        if $v == 0 {
            None
        } else {
            Some(AppState::$cname($fname::from_repr($v - 1).unwrap()))
        }
    };
}

impl AppInfo {
    /// Goes to the next menu depending on the position in the program
    fn next_menu(&mut self, direction: InputDirection) {
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

// impl SelectedFinalConfig {
//     fn change_menu(&mut self, direction: InputDirection) {
//         match direction {
//             InputDirection::Right => match self {
//                 SelectedFinalConfig::StartFreq => Self::FinalFreq,
//                 SelectedFinalConfig::FinalFreq => Self::Points,
//                 SelectedFinalConfig::Points => Self::Proceed,
//                 SelectedFinalConfig::Proceed => (),
//             },
//             InputDirection::Left => todo!(),
//             _ => (),
//         }
//     }
// }

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
    // TODO: Remove the clone part
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

impl AppInfo {
    /// Create a new app with the port info available
    pub(crate) fn new(info: Vec<SerialPortInfo>) -> Self {
        Self {
            should_quit: false,
            baudrate: BaudRateList::default(),
            port_list: PortList::from_serial(info),
            flow_state: AppState::default(),
            status_message: String::new(),
        }
    }

    /// Handle keys while in the first menu
    fn handle_start_keys(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.change_cursor_position(InputDirection::Down),
            KeyCode::Char('k') | KeyCode::Up => self.change_cursor_position(InputDirection::Up),
            KeyCode::Char('h') | KeyCode::Left => self.next_menu(InputDirection::Left),
            KeyCode::Char('l') | KeyCode::Right => self.next_menu(InputDirection::Right),
            // KeyCode::Char('g') | KeyCode::Home => self.select_first(),
            // KeyCode::Char('G') | KeyCode::End => self.select_last(),
            // KeyCode::Enter => {
            //     self.toggle_status();
            // }
            _ => {}
        }
    }

    /// Contains the main loop for running the app
    pub(crate) fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|v| self.draw_config(v))?;
            if let Event::Key(key) = event::read()? {
                self.handle_start_keys(key);
            };
            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    /// Initialisation of the config window
    fn init_config(&mut self) {
        // self.flow_state = AppState::InitialConfig();
    }

    /// Draw the config info
    // TODO: This config doesn't need to be reinit every frame
    fn draw_config(&mut self, frame: &mut Frame) {
        let [title_area, main_area, status_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(10),
            // Constraint::Min(8),
            // Constraint::Length(3),
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
        let (b_color, p_color) = match self.flow_state {
            AppState::InitialConfig(SelectedConfig::Port) => (UNSELECTED_COLOR, SELECTED_COLOR),
            AppState::InitialConfig(SelectedConfig::Baudrate) => (SELECTED_COLOR, UNSELECTED_COLOR),
            _ => unreachable!(),
        };
        let list_ports = List::new(self.port_list.to_vecstr())
            .block(Block::bordered().title("COM Ports").border_style(p_color))
            .style(Style::default().fg(p_color))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .bg(Color::Blue),
            )
            .repeat_highlight_symbol(true);
        let list_baudrates = List::new(self.baudrate.to_vecstr())
            .block(Block::bordered().title("Baudrates").border_style(b_color))
            .style(Style::default().fg(b_color))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .bg(Color::Blue),
            )
            .repeat_highlight_symbol(true);
        self.status_message = format!("{:?} ", self.flow_state);
        frame.render_stateful_widget(list_ports, area_ports, &mut self.port_list.state);
        frame.render_stateful_widget(list_baudrates, area_baud, &mut self.baudrate.state);
        frame.render_widget(Paragraph::new(self.status_message.clone()), title_area);
        self.render_footer(frame, status_area)
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

    /// A directionnal input was received
    fn change_cursor_position(&mut self, direction: InputDirection) {
        //Where in the menus are we
        match self.flow_state {
            AppState::InitialConfig(ref selected_menu) => {
                // self.status_message = format!(
                //     "{:?} {:?} {:?}",
                //     selected_menu, self.baudrate.state, self.port_list.state
                // );
                match selected_menu {
                    SelectedConfig::Baudrate => self.baudrate.change_cursor(direction),
                    SelectedConfig::Port => self.port_list.change_cursor(direction),
                }
            }
            AppState::FinalConfig(ref selected_menu) => match selected_menu {
                SelectedFinalConfig::StartFreq => todo!(),
                SelectedFinalConfig::FinalFreq => todo!(),
                SelectedFinalConfig::Points => todo!(),
            },
            AppState::Popup => (),
            AppState::Finished => (),
        }
    }

    // /// Change the selected menu
    // fn change_menu(&mut self, direction: InputDirection) {
    //     match self.flow_state {
    //         AppState::InitialConfig(ref menu) => self,
    //         AppState::FinalConfig(_) => todo!(),
    //         AppState::Popup => (),
    //         AppState::Finished => (),
    //     }
    // } // Handle code after validation
}

#[allow(clippy::missing_docs_in_private_items)]
impl AppInfo {}

#[allow(clippy::missing_docs_in_private_items)]
const fn alternate_colors(i: usize) -> Color {
    if i % 2 == 0 {
        NORMAL_ROW_BG
    } else {
        ALT_ROW_BG_COLOR
    }
}
