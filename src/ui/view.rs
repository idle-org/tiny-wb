//! Ratatui rendering; acquisition results arrive through shared scan state.

use super::state::{
    App, AppState, Plot, SelectedConnection, SelectedScan, SELECTED_COLOR, UNSELECTED_COLOR,
};
use crate::error::UiError;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::Marker,
    widgets::{Axis, Block, Chart, Clear, Dataset, GraphType, List, Paragraph, Wrap},
    Frame,
};

impl App {
    /// Render the active screen; lifecycle transitions happen in the controller.
    pub(super) fn draw(&mut self, frame: &mut Frame<'_>) -> Result<(), UiError> {
        match self.flow_state {
            AppState::SelectingConnection(_) => self.draw_config(frame),
            AppState::SelectingScan(_) => self.draw_final_config(frame),
            AppState::Scanning => self.draw_progress(frame),
            AppState::Live => self.draw_live(frame),
            AppState::Complete => self.draw_end_window(frame),
            AppState::Error => self.draw_error(frame),
        }
    }

    /// Render the newest complete live snapshot while the worker continues.
    fn draw_live(&mut self, frame: &mut Frame<'_>) -> Result<(), UiError> {
        let [_, main, _] = Layout::horizontal([
            Constraint::Min(2),
            Constraint::Percentage(96),
            Constraint::Min(2),
        ])
        .areas(frame.area());
        let [status_area, graph_area, footer_area] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .areas(main);
        let status = format!(
            "{} — {}\n{}",
            self.live_state.mode(),
            self.live_state.message(),
            self.live_state.info()
        );
        frame.render_widget(
            Paragraph::new(status)
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title("Live capture")),
            status_area,
        );
        if let Some(plot) = self.plot.as_ref() {
            let bounds = plot.viewport.bounds();
            let span = bounds[1] - bounds[0];
            let midpoint = (bounds[0] + bounds[1]) / 2.0;
            let chart = Chart::new(vec![Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(Style::new().yellow())
                .data(&plot.data)])
            .x_axis(
                Axis::default()
                    .title("Frequency (MHz)")
                    .bounds(bounds)
                    .labels([
                        format_frequency(bounds[0], span),
                        format_frequency(midpoint, span),
                        format_frequency(bounds[1], span),
                    ]),
            )
            .y_axis(
                Axis::default()
                    .title("Level (dBm)")
                    .bounds([self.config.ui.chart_min_dbm, self.config.ui.chart_max_dbm]),
            );
            frame.render_widget(chart, graph_area);
            if self.show_peak_info {
                render_peak_info(frame, graph_area, plot);
            }
        } else {
            frame.render_widget(
                Paragraph::new("Waiting for the first complete sweep…")
                    .centered()
                    .block(Block::bordered()),
                graph_area,
            );
        }
        frame.render_widget(
            Paragraph::new(
                "1–6 trace · zoom +/= / - · pan ←/→ · 0 reset · m peak · s save · q stop+save",
            )
            .centered()
            .wrap(Wrap { trim: true }),
            footer_area,
        );
        Ok(())
    }

    /// Render scan settings with the focused field highlighted.
    fn draw_final_config(&mut self, frame: &mut Frame<'_>) -> Result<(), UiError> {
        let [_, spaced, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(100),
            Constraint::Min(7),
        ])
        .areas(frame.area());
        let [_, title_area, _, main_area, _, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(spaced);
        let [area_minf, area_maxf, area_points, repeat] = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .areas(main_area);
        let (mif_color, maf_color, poi_color, rpt_color) = match self.flow_state {
            AppState::SelectingScan(SelectedScan::StartFreq) => (
                SELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::SelectingScan(SelectedScan::FinalFreq) => (
                UNSELECTED_COLOR,
                SELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::SelectingScan(SelectedScan::Points) => (
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                SELECTED_COLOR,
                UNSELECTED_COLOR,
            ),
            AppState::SelectingScan(SelectedScan::Repeat) => (
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                UNSELECTED_COLOR,
                SELECTED_COLOR,
            ),
            _ => unreachable!(),
        };

        frame.render_widget(
            Paragraph::new(self.start_frequency.to_string())
                .block(Block::bordered().title("Start Frequency"))
                .style(mif_color),
            area_minf,
        );
        frame.render_widget(
            Paragraph::new(self.stop_frequency.to_string())
                .block(Block::bordered().title("Stop Frequency"))
                .style(maf_color),
            area_maxf,
        );
        frame.render_widget(
            Paragraph::new(self.points.to_string())
                .block(Block::bordered().title("Points"))
                .style(poi_color),
            area_points,
        );
        frame.render_widget(
            Paragraph::new(self.repeat.to_string())
                .block(Block::bordered().title("Repeat"))
                .style(rpt_color),
            repeat,
        );

        self.update_color()?;
        frame.render_widget(
            Paragraph::new(self.status_message.clone())
                .wrap(Wrap { trim: true })
                .block(Block::bordered()),
            title_area,
        );
        self.render_footer(frame, status_area);
        Ok(())
    }
    /// Show keyboard guidance below editable settings.
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let guidance = match self.flow_state {
            AppState::SelectingScan(SelectedScan::StartFreq | SelectedScan::FinalFreq)
                if self.start_frequency.is_editing() || self.stop_frequency.is_editing() =>
            {
                "Type MHz; Backspace deletes, ↲ applies, and Esc cancels."
            }
            AppState::SelectingScan(SelectedScan::Points) if self.points.is_editing() => {
                "Type the point count; Backspace deletes, ↲ applies, and Esc cancels."
            }
            AppState::SelectingScan(_) => {
                "Use ↓↑ to change, ←→ to select, digits to type MHz/points, and ↲ to continue."
            }
            _ => "Use ↓↑ to move, ←→ to select, and ↲ to continue.",
        };
        frame.render_widget(Paragraph::new(guidance).centered(), area);
    }

    /// Render the available ports and configured baud choices.
    fn draw_config(&mut self, frame: &mut Frame) -> Result<(), UiError> {
        let [_, spaced, _] = Layout::horizontal([
            Constraint::Min(7),
            Constraint::Percentage(100),
            Constraint::Min(7),
        ])
        .areas(frame.area());
        let [_, title_area, _, main_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .areas(spaced);
        let [area_ports, area_baud] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(main_area);
        let list_ports = self
            .menu_items
            .portlist
            .as_ref()
            .ok_or(UiError::UninitializedItemList("Port", line!(), file!()))?;
        let list_baudrates = self
            .menu_items
            .baudlist
            .as_ref()
            .ok_or(UiError::UninitializedItemList("Baud", line!(), file!()))?;
        frame.render_stateful_widget(list_ports, area_ports, &mut self.port_list.state);
        frame.render_stateful_widget(list_baudrates, area_baud, &mut self.baudrate.state);
        frame.render_widget(
            Paragraph::new(self.status_message.clone())
                .wrap(Wrap { trim: true })
                .block(Block::bordered()),
            title_area,
        );
        self.render_footer(frame, status_area);
        Ok(())
    }

    /// Render a snapshot of worker progress without consuming completed samples.
    fn draw_progress(&mut self, frame: &mut Frame<'_>) -> Result<(), UiError> {
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
            let status = self.scan_state.message();
            let status = Paragraph::new(status.clone())
                .block(Block::bordered().title("Operation"))
                .style(SELECTED_COLOR);
            let message = self.scan_state.info();
            let message = Paragraph::new(message.clone())
                .wrap(Wrap { trim: true })
                .block(Block::bordered().title("TinySA Info"))
                .style(SELECTED_COLOR);
            let pb = self
                .menu_items
                .progress_bar
                .take()
                .ok_or(UiError::UninitializedItemList(
                    "ProgressBar",
                    line!(),
                    file!(),
                ))?
                .ratio(self.scan_state.progress() as f64 / 100.);
            (pb, status, message)
        };
        frame.render_widget(pb.clone(), progress_area);
        frame.render_widget(status, title_area);
        frame.render_widget(message, main_area);
        self.menu_items.progress_bar = Some(pb);
        Ok(())
    }

    /// Render the original failure context and the last worker operation.
    fn draw_error(&mut self, frame: &mut Frame) -> Result<(), UiError> {
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

        let conn_status = self.scan_state.status();
        let status_message = self.error_message_from_current_error();
        let status = Paragraph::new(status_message)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title("Status"))
            .style(Color::Red);
        let additional = self.scan_state.error().unwrap_or_default();
        let error = Paragraph::new(format!(
            "Last status: {:?}\nLast message: {}\n{}",
            conn_status,
            self.scan_state.message(),
            additional
        ))
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Status"));
        let footer = Paragraph::new("Press ↲ or q to exit".to_string()).centered();
        frame.render_widget(status, title_area);
        frame.render_widget(error, message_area);
        frame.render_widget(footer, footer_area);
        Ok(())
    }

    /// Display the original error context rather than maintaining a second mapping.
    fn error_message_from_current_error(&self) -> String {
        self.scan_state
            .error()
            .unwrap_or_else(|| "An unspecified error occurred".into())
    }

    /// Plot completed samples using the configured level bounds.
    fn draw_end_window(&mut self, frame: &mut Frame) -> Result<(), UiError> {
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

        let plot = self.plot.as_ref().ok_or(UiError::NoReturnData)?;
        let bounds = plot.viewport.bounds();
        let span = bounds[1] - bounds[0];
        let status = Paragraph::new(format!(
            "{} — {} points — visible {} to {} MHz",
            plot.label,
            plot.data.len(),
            format_frequency(bounds[0], span),
            format_frequency(bounds[1], span),
        ))
        .block(Block::bordered().title("Status"))
        .style(SELECTED_COLOR);

        let datasets = vec![Dataset::default()
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::new().yellow())
            .data(&plot.data)];

        let midpoint = (bounds[0] + bounds[1]) / 2.0;

        let chart = Chart::new(datasets)
            .x_axis(
                Axis::default()
                    .title("Frequency (MHz)")
                    .bounds(bounds)
                    .style(Style::default().fg(Color::Gray))
                    .labels([
                        format_frequency(bounds[0], span),
                        format_frequency(midpoint, span),
                        format_frequency(bounds[1], span),
                    ]),
            )
            .y_axis(
                Axis::default()
                    .bounds([self.config.ui.chart_min_dbm, self.config.ui.chart_max_dbm])
                    .style(Style::default().fg(Color::Gray))
                    .labels([
                        format!("{} dBm", self.config.ui.chart_min_dbm),
                        format!(
                            "{} dBm",
                            (self.config.ui.chart_min_dbm + self.config.ui.chart_max_dbm) / 2.0
                        ),
                        format!("{} dBm", self.config.ui.chart_max_dbm),
                    ]),
            );
        frame.render_widget(chart, graph_area);
        if self.show_peak_info {
            render_peak_info(frame, graph_area, plot);
        }
        let keys = &self.config.ui.bindings;
        let restart = if self.can_acquire {
            "; r repeats; n edits a new scan"
        } else {
            ""
        };
        let footer = Paragraph::new(format!(
            "zoom {} / {}; pan {} / {}; reset {}; m peak{}; q exits",
            keys.zoom_in.join("|"),
            keys.zoom_out.join("|"),
            keys.pan_left.join("|"),
            keys.pan_right.join("|"),
            keys.reset_view.join("|"),
            restart,
        ))
        .centered();
        frame.render_widget(status, title_area);
        frame.render_widget(footer, footer_area);
        Ok(())
    }
}

/// Overlay the maximum visible sample without changing the chart's data bounds.
fn render_peak_info(frame: &mut Frame<'_>, graph_area: Rect, plot: &Plot) {
    let Some((frequency, level)) = plot.visible_peak() else {
        return;
    };
    let width = graph_area.width.min(26);
    let height = graph_area.height.min(4);
    if width == 0 || height == 0 {
        return;
    }
    let area = Rect::new(
        graph_area.right().saturating_sub(width),
        graph_area.y,
        width,
        height,
    );
    let span = plot.viewport.bounds()[1] - plot.viewport.bounds()[0];
    let text = format!("{level:.2} dBm\n{} MHz", format_frequency(frequency, span));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).block(Block::bordered().title("Visible peak")),
        area,
    );
}

/// Keep axis labels compact while retaining useful precision after zooming.
fn format_frequency(value: f64, visible_span: f64) -> String {
    let tick_spacing = visible_span / 2.0;
    let precision = if tick_spacing >= 1.0 {
        2
    } else {
        ((-tick_spacing.log10()).ceil() as usize).clamp(2, 6)
    };
    format!("{value:.precision$}")
}

impl App {
    /// Apply the active/inactive style to a cached list widget.
    pub(crate) fn change_color(
        &mut self,
        name: &'static str,
        list: List<'static>,
        color: Color,
    ) -> Option<List<'static>> {
        Some(
            list.block(Block::bordered().title(name).border_style(color))
                .style(Style::default().fg(color)),
        )
    }
}

impl App {
    /// Update connection selection styling after navigation.
    pub(crate) fn update_color(&mut self) -> Result<(), UiError> {
        match self.flow_state {
            AppState::SelectingConnection(ref c) => {
                let (p_c, b_c) = match c {
                    SelectedConnection::Baudrate => (UNSELECTED_COLOR, SELECTED_COLOR),
                    SelectedConnection::Port => (SELECTED_COLOR, UNSELECTED_COLOR),
                };
                let (p, b) = (
                    self.menu_items
                        .portlist
                        .take()
                        .ok_or(UiError::UninitializedItemList(
                            "Port List",
                            line!(),
                            file!(),
                        ))?,
                    self.menu_items
                        .baudlist
                        .take()
                        .ok_or(UiError::UninitializedItemList(
                            "Port List",
                            line!(),
                            file!(),
                        ))?,
                );
                self.menu_items.portlist = self.change_color("COM Ports", p, p_c);
                self.menu_items.baudlist = self.change_color("Baudrate", b, b_c);
            }
            AppState::SelectingScan(_selected_final_config) => (),
            _ => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::format_frequency;

    #[test]
    fn frequency_labels_gain_precision_as_the_view_narrows() {
        assert_eq!(format_frequency(240.123_456, 100.0), "240.12");
        assert_eq!(format_frequency(240.123_456, 0.001), "240.1235");
        assert_eq!(format_frequency(240.123_456, 0.000_001), "240.123456");
    }
}
