use super::super::app::{App, Focus, SelectedTab};
use super::{bottom_bar, channels, functions, inspect, logs, samples, top_bar};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph, Tabs},
    Frame,
};

pub(crate) fn render_ui(frame: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs
            Constraint::Length(3), // Status bar
            Constraint::Min(0),    // Main content area
            Constraint::Length(3), // Help bar
        ])
        .split(frame.area());

    // Render tabs
    render_tabs(frame, main_chunks[0], app.selected_tab);

    let has_data = match app.selected_tab {
        SelectedTab::Metrics => !app.metrics.data.0.is_empty(),
        SelectedTab::Channels => !app.channels.channels.is_empty(),
    };

    top_bar::render_status_bar(
        frame,
        main_chunks[1],
        app.paused,
        app.last_successful_fetch,
        app.error_message.is_some(),
        has_data,
    );

    // Render content based on selected tab
    match app.selected_tab {
        SelectedTab::Metrics => {
            if app.show_samples {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(main_chunks[2]);

                functions::render_functions_table(frame, app, content_chunks[0]);
                samples::render_samples_panel(frame, content_chunks[1], app);
            } else {
                functions::render_functions_table(frame, app, main_chunks[2]);
            }
        }
        SelectedTab::Channels => {
            render_channels_view(frame, app, main_chunks[2]);
        }
    }

    bottom_bar::render_help_bar(frame, main_chunks[3], app.selected_tab, app.focus);
}

/// Renders the channels view including the main table, logs panel, and error states
fn render_channels_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = &app.channels.channels;

    if let Some(ref error_msg) = app.error_message {
        if stats.is_empty() {
            let error_text = vec![
                Line::from(""),
                Line::from("Error").red().bold().centered(),
                Line::from(""),
                Line::from(error_msg.as_str()).red().centered(),
                Line::from(""),
                Line::from(format!(
                    "Make sure the metrics server is running on http://127.0.0.1:{}",
                    app.metrics_port
                ))
                .yellow()
                .centered(),
            ];

            let block = Block::bordered().border_set(border::THICK);
            frame.render_widget(Paragraph::new(error_text).block(block), area);
            return;
        }
    }

    if stats.is_empty() {
        let empty_text = vec![
            Line::from(""),
            Line::from("No channel statistics found")
                .yellow()
                .centered(),
            Line::from(""),
            Line::from("Make sure channels are instrumented and the server is running").centered(),
        ];

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_text).block(block), area);
        return;
    }

    // Split the area if logs are being shown
    let (table_area, logs_area) = if app.show_logs {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let selected_index = app.table_state.selected().unwrap_or(0);
    let channel_position = selected_index + 1; // 1-indexed
    let total_channels = stats.len();

    channels::render_channels_panel(
        stats,
        table_area,
        frame,
        &mut app.table_state,
        app.show_logs,
        app.focus,
        channel_position,
        total_channels,
    );

    // Render logs panel if visible
    if let Some(logs_area) = logs_area {
        let channel_label = app
            .table_state
            .selected()
            .and_then(|i| stats.get(i))
            .map(|stat| {
                if stat.label.is_empty() {
                    stat.id.to_string()
                } else {
                    stat.label.clone()
                }
            })
            .unwrap_or_else(|| "Unknown".to_string());

        if let Some(ref cached_logs) = app.logs {
            let has_missing_log = cached_logs
                .logs
                .sent_logs
                .iter()
                .any(|entry| entry.message.is_none());
            let display_label = if has_missing_log {
                format!("{} (missing \"log = true\")", channel_label)
            } else {
                channel_label
            };
            logs::render_logs_panel(
                cached_logs,
                &display_label,
                logs_area,
                frame,
                &mut app.logs_table_state,
                app.focus == Focus::Logs,
                app.channels.current_elapsed_ns,
            );
        } else {
            let message = if app.paused {
                "(refresh paused)"
            } else if app.error_message.is_some() {
                "(cannot fetch new data)"
            } else {
                "(no data)"
            };
            logs::render_logs_placeholder(&channel_label, message, logs_area, frame);
        }
    }

    if app.focus == Focus::Inspect {
        if let Some(ref inspected_log) = app.inspected_log {
            inspect::render_inspect_popup(inspected_log, area, frame);
        }
    }
}

fn render_tabs(frame: &mut Frame, area: ratatui::layout::Rect, selected_tab: SelectedTab) {
    let titles = vec![
        Line::from(SelectedTab::Metrics.title()).style(
            Style::default()
                .fg(if selected_tab == SelectedTab::Metrics {
                    Color::Yellow
                } else {
                    Color::Gray
                })
                .add_modifier(if selected_tab == SelectedTab::Metrics {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Line::from(SelectedTab::Channels.title()).style(
            Style::default()
                .fg(if selected_tab == SelectedTab::Channels {
                    Color::Yellow
                } else {
                    Color::Gray
                })
                .add_modifier(if selected_tab == SelectedTab::Channels {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
    ];

    let selected_index = match selected_tab {
        SelectedTab::Metrics => 0,
        SelectedTab::Channels => 1,
    };

    let tabs = Tabs::new(titles)
        .select(selected_index)
        .divider(" | ")
        .style(Style::default());

    frame.render_widget(tabs, area);
}
