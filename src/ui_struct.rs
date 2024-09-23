//! All ui structures and utility functions

//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use clap::builder::Resettable;
use color_eyre::{owo_colors::OwoColorize, Result};
use ratatui::{
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        style::Print,
    },
    layout::{Constraint, Direction, Layout, Rect},
    prelude::BlockExt,
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Styled, Stylize,
    },
    symbols,
    text::Line,
    widgets::{
        Block, Borders, Chart, HighlightSpacing, LineGauge, List, ListDirection, ListItem,
        ListState, Padding, Paragraph, ScrollDirection, StatefulWidget, Widget, Wrap,
    },
    DefaultTerminal, Frame,
};
use serialport::SerialPortInfo;
use std::{
    borrow::{Borrow, BorrowMut},
    cmp::min,
    fmt::Pointer,
    iter::Iterator,
    ops::Add,
    sync::{Arc, Mutex},
    time::Instant,
};
use strum::{EnumCount, EnumIter, EnumProperty};
use strum::{FromRepr, IntoEnumIterator};
use tui_textarea::{Input, Key, TextArea};

use crate::structs::{InputDirection, TinyUIError};

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
pub(crate) const SELECTED_COLOR: Color = Color::White;
/// Color of an unselected item
pub(crate) const UNSELECTED_COLOR: Color = Color::DarkGray;

/// This struct holds the current state of the app.
#[derive(Debug)]
pub(crate) struct AppInfo<'a> {
    /// Wether the app sould immediately quit
    pub(crate) should_quit: bool,
    /// The list of baudrates
    pub(crate) baudrate: BaudRateList,
    /// The list of ports
    pub(crate) port_list: PortList,
    /// State of the configuration, akin to tab or window
    pub(crate) flow_state: AppState,
    /// The list of menu item generated
    pub(crate) menu_items: MenuItems<'a>,
    /// A debug message string
    pub(crate) status_message: String,
    /// The start frequency
    pub(crate) start_frequency: UIUsize,
    /// The stop frequency
    pub(crate) stop_frequency: UIUsize,
    /// The number of points
    pub(crate) points: UIUsize,
    /// Global progress
    pub(crate) progress: Arc<Mutex<usize>>,
}

#[derive(Debug)]
pub(crate) struct UIUsize {
    /// Current value held by the usize
    value: usize,
    /// Increment by witch it can be upped
    increment: usize,
    /// Max value to clamp it to
    clamp: usize,
}

impl UIUsize {
    /// Change the value depending on selection
    pub(crate) fn change_value(&mut self, direction: InputDirection) {
        match direction {
            InputDirection::Up => self.value = self.value.add(self.increment).clamp(0, self.clamp),
            InputDirection::Down => self.value = self.value.saturating_sub(self.increment),
            _ => (),
        }
    }

    /// Create a new UIUsize, usefull for TUI manipulation
    pub(crate) fn new(value: usize, increment: usize, clamp: usize) -> Self {
        Self {
            value,
            increment,
            clamp,
        }
    }

    /// Convert to a string with the litteral value
    pub(crate) fn int_to_string(&self) -> String {
        format!("{}", self.value)
    }

    /// Convert to mhz multiplying it by 1_000_000
    pub(crate) fn mhz_to_string(&self) -> String {
        format!("{}", self.value * 1000000)
    }
}

#[derive(Debug)]
/// The inner state of the app, useful for display and keystrokes
pub(crate) enum AppState {
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

#[macro_export]
/// Shortcut for move right
macro_rules! mright {
    ($v:ident, $cname:ident,$fname:ident) => {
        Some(AppState::$cname(
            $fname::from_repr(min(($v + 1), $fname::COUNT - 1))
                .ok_or(TinyUIError::MenuSelectionError)?,
        ))
    };
}

#[macro_export]
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
/// The selected item in the config
#[derive(Debug, EnumCount, FromRepr, Clone, Copy)]
pub(crate) enum SelectedConfig {
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
pub(crate) enum SelectedFinalConfig {
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
pub(crate) struct BaudRateList {
    /// The selected baudrate (default : BAUD1115200)
    pub(crate) selected: usize,
    /// The baudrate list
    pub(crate) baudrates: Vec<BaudRate>,
    /// The inner state of the widget
    pub(crate) state: ListState,
}
impl BaudRateList {
    /// Convert into a vec of strings, this is extremely expensive and shouldn't be done that way
    pub(crate) fn to_vecstr(&self) -> Vec<String> {
        self.baudrates
            .iter()
            .map(|i| format!("{}", *i as usize))
            .collect()
    }

    /// Change the cursor position on the baudrates
    pub(crate) fn change_cursor(&mut self, direction: InputDirection) {
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
pub(crate) enum BaudRate {
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
    pub(crate) ports: Vec<String>,
    /// The currently selected port (doing it that way completly avoids the )
    pub(crate) selected: usize,
    /// The inner state of the widget
    pub(crate) state: ListState,
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
    pub(crate) fn to_vecstr(&self) -> Vec<String> {
        self.ports.clone()
    }

    /// Change the selected item
    pub(crate) fn change_cursor(&mut self, direction: InputDirection) {
        match direction {
            InputDirection::Up => self.state.select_previous(),
            InputDirection::Down => self.state.select_next(),
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
/// The static list of all menu items
pub(crate) struct MenuItems<'a> {
    /// List of port items
    pub(crate) portlist: Option<List<'a>>,
    /// List of port items
    pub(crate) baudlist: Option<List<'a>>,
    /// List of all the textboxes
    pub(crate) textboxes: Vec<TextArea<'a>>,
    /// The progress bar
    pub(crate) progress_bar: Option<LineGauge<'a>>,
    /// Controller info
    pub(crate) info: Option<Chart<'a>>,
    /// The final graph
    pub(crate) graph: Option<Chart<'a>>,
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
// #[allow(clippy::missing_docs_in_private_items)]
// const fn alternate_colors(i: usize) -> Color {
//     if i % 2 == 0 {
//         NORMAL_ROW_BG
//     } else {
//         ALT_ROW_BG_COLOR
//     }
// }
impl<'a> AppInfo<'a> {
    /// Put an item a specified color
    pub(crate) fn change_color(
        &mut self,
        name: &'a str,
        list: List<'a>,
        color: Color,
    ) -> Option<List<'a>> {
        Some(
            list.block(Block::bordered().title(name).border_style(color))
                .style(Style::default().fg(color)),
        )
    }
}
impl<'a> AppInfo<'a> {
    /// Update all the colors
    pub(crate) fn update_color(&mut self) -> Result<(), TinyUIError> {
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
                // let (mif_color, maf_color, poi_color) = match self.flow_state {
                let colorlist = match c {
                    SelectedFinalConfig::StartFreq => {
                        [SELECTED_COLOR, UNSELECTED_COLOR, UNSELECTED_COLOR]
                    }
                    SelectedFinalConfig::FinalFreq => {
                        [UNSELECTED_COLOR, SELECTED_COLOR, UNSELECTED_COLOR]
                    }
                    SelectedFinalConfig::Points => {
                        [UNSELECTED_COLOR, UNSELECTED_COLOR, SELECTED_COLOR]
                    }
                };

                for (area, color) in self.menu_items.textboxes.iter_mut().zip(colorlist) {
                    area.set_block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().fg(color))
                            .title(" Inactive (^X to switch) "),
                    )
                }
            }
            AppState::Popup => todo!(),
            AppState::Finished => todo!(),
        }
        Ok(())
    }
}
