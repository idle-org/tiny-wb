//! The (minimal) user interface to control the program
#![allow(unused_imports)]
use color_eyre::{owo_colors::OwoColorize, Result};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{
        palette::tailwind::{BLUE, GREEN, SLATE},
        Color, Modifier, Style, Stylize,
    },
    symbols,
    text::Line,
    widgets::{
        Block, Borders, HighlightSpacing, List, ListDirection, ListItem, ListState, Padding,
        Paragraph, StatefulWidget, Widget, Wrap,
    },
    DefaultTerminal, Frame,
};
use serialport::SerialPortInfo;
use strum::EnumIter;
use strum::IntoEnumIterator;

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

/// This struct holds the current state of the app.
#[derive(Debug)]
pub(crate) struct AppInfo {
    /// Wether the app sould immediately quit
    should_quit: bool,
    /// The list of baudrates
    baudrate: BaudRateList,
    /// The list of ports
    port_list: PortList,
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
    // fn to_itemlist(&self) -> Vec<ListItem> {
    //     self.baudrates
    //         .iter()
    //         .map(|i| ListItemformat!("{}", *i as usize))
    //         .collect()
    // }

    /// Pick the baudrate above
    fn above(&mut self) {
        // self.selected = (self.selected + 1) % self.baudrates.len();
        self.state.select_previous();
    }

    /// Pick the baudrate below
    fn below(&mut self) {
        // self.selected = (self.selected - 1) % self.baudrates.len();
        self.state.select_next();
    }
}

impl Default for BaudRateList {
    fn default() -> Self {
        Self {
            selected: 3,
            baudrates: BaudRate::iter().collect::<Vec<_>>(),
            state: ListState::default(),
        }
    }
}

// impl Iterator for BaudRateList {
//     type Item = &'str;

//     fn next(&mut self) -> Option<Self::Item> {
//         todo!()
//     }
// }
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
            state: ListState::default(),
        }
    }

    /// Convert the list of ports
    /// TODO: Remove the clone part
    fn to_vecstr(&self) -> Vec<String> {
        self.ports.clone()
    }
}

/// Try to guess the best port to preselect
fn find_best_port(ports: &[String]) -> usize {
    let best_ports = ["/dev/ttyACM0", "COM3", "COM5"];
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
        }
    }

    /// Handle keys while in the first menu
    fn handle_start_keys(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // KeyCode::Char('h') | KeyCode::Left => self.baudrate.
            KeyCode::Char('j') | KeyCode::Down => self.baudrate.above(),
            KeyCode::Char('k') | KeyCode::Up => self.baudrate.below(),
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

    // fn init_config(&self) -> &Buffer {
    //     let [_title_area, main_area, _status_area] = Layout::vertical([
    //         Constraint::Length(3),
    //         Constraint::Min(10),
    //         Constraint::Length(3),
    //     ])
    //     .areas(frame.area());
    //     let [_, area_ports, area_baud, _] = Layout::horizontal([
    //         Constraint::Min(10),
    //         Constraint::Percentage(50),
    //         Constraint::Percentage(50),
    //         Constraint::Min(10),
    //     ])
    //     .areas(main_area);
    //     main_area
    // }

    /// Draw the config info
    // TODO: This config doesn't need to be reinit every frame
    fn draw_config(&mut self, frame: &mut Frame) {
        let [_title_area, main_area, _status_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .areas(frame.area());
        let [_, area_ports, area_baud, _] = Layout::horizontal([
            Constraint::Min(10),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            Constraint::Min(10),
        ])
        .areas(main_area);

        let list_baudrates = List::new(self.baudrate.to_vecstr())
            .block(Block::bordered().title("Baudrates"))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);
        let list_ports = List::new(self.port_list.to_vecstr())
            .block(Block::bordered().title("COM Ports"))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::UNDERLINED))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);

        // frame.render_widget(list_ports, area_ports);
        // frame.render_widget(list_baudrates, area_baud);
        frame.render_stateful_widget(list_baudrates, area_baud, &mut self.baudrate.state);
        frame.render_stateful_widget(list_ports, area_ports, &mut self.port_list.state);
        // self.render_footer(_status_area, frame);
    }

    /// Render the selection footer
    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(
            "Use ↓↑ to move, ←→ to change status widget and ↲ to validate configuration.",
        )
        .centered()
        .render(area, buf);
    } // Handle code after validation
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
