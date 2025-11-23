use super::super::app::{App, ChannelsFocus, FunctionsFocus, SelectedTab, StreamsFocus};
use super::channels::{inspect, logs as channel_logs};
use super::functions_memory::logs as memory_logs;
use super::functions_timing::logs as timing_logs;
use super::streams::{inspect as stream_inspect, logs as stream_logs};
use super::{bottom_bar, channels, functions_memory, functions_timing, streams, top_bar};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Paragraph, Tabs},
    Frame,
};

#[cfg_attr(feature = "hotpath", hotpath::measure)]
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

    let has_data = match app.selected_tab {
        SelectedTab::Timing => !app.timing_functions.data.0.is_empty(),
        SelectedTab::Memory => !app.memory_functions.data.0.is_empty(),
        SelectedTab::Channels => !app.channels.channels.is_empty(),
        SelectedTab::Streams => !app.streams.streams.is_empty(),
    };

    top_bar::render_status_bar(
        frame,
        main_chunks[1],
        app.paused,
        app.last_successful_fetch,
        app.error_message.is_some(),
        has_data,
    );

    render_tabs(frame, main_chunks[0], app.selected_tab);

    // Render content based on selected tab
    match app.selected_tab {
        SelectedTab::Timing => {
            if app.show_function_logs {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(main_chunks[2]);

                functions_timing::render_functions_table(frame, app, content_chunks[0]);
                timing_logs::render_function_logs_panel(
                    app.current_function_logs.as_ref(),
                    app.selected_function_name().as_deref(),
                    &app.timing_functions.hotpath_profiling_mode,
                    app.timing_functions.total_elapsed,
                    content_chunks[1],
                    frame,
                    &mut app.function_logs_table_state,
                    app.functions_focus == FunctionsFocus::Logs,
                );
            } else {
                functions_timing::render_functions_table(frame, app, main_chunks[2]);
            }
        }
        SelectedTab::Memory => {
            if app.show_function_logs {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(main_chunks[2]);

                functions_memory::render_functions_table(frame, app, content_chunks[0]);
                memory_logs::render_function_logs_panel(
                    app.current_function_logs.as_ref(),
                    app.selected_function_name().as_deref(),
                    &app.memory_functions.hotpath_profiling_mode,
                    app.memory_functions.total_elapsed,
                    content_chunks[1],
                    frame,
                    &mut app.function_logs_table_state,
                    app.functions_focus == FunctionsFocus::Logs,
                );
            } else {
                functions_memory::render_functions_table(frame, app, main_chunks[2]);
            }
        }
        SelectedTab::Channels => {
            render_channels_view(frame, app, main_chunks[2]);
        }
        SelectedTab::Streams => {
            render_streams_view(frame, app, main_chunks[2]);
        }
    }

    bottom_bar::render_help_bar(
        frame,
        main_chunks[3],
        app.selected_tab,
        app.channels_focus,
        app.streams_focus,
        app.functions_focus,
    );
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
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

    let selected_index = app.channels_table_state.selected().unwrap_or(0);
    let channel_position = selected_index + 1; // 1-indexed
    let total_channels = stats.len();

    channels::render_channels_panel(
        stats,
        table_area,
        frame,
        &mut app.channels_table_state,
        app.show_logs,
        app.channels_focus,
        channel_position,
        total_channels,
    );

    // Render logs panel if visible
    if let Some(logs_area) = logs_area {
        let channel_label = app
            .channels_table_state
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
            channel_logs::render_logs_panel(
                cached_logs,
                &display_label,
                logs_area,
                frame,
                &mut app.channel_logs_table_state,
                app.channels_focus == ChannelsFocus::Logs,
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
            channel_logs::render_logs_placeholder(&channel_label, message, logs_area, frame);
        }
    }

    if app.channels_focus == ChannelsFocus::Inspect {
        if let Some(ref inspected_log) = app.inspected_log {
            inspect::render_inspect_popup(inspected_log, area, frame);
        }
    }
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn render_streams_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = &app.streams.streams;

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
            Line::from("No stream statistics found").yellow().centered(),
            Line::from(""),
            Line::from("Make sure streams are instrumented and the server is running").centered(),
        ];

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_text).block(block), area);
        return;
    }

    // Split the area if logs are being shown
    let (table_area, logs_area) = if app.show_stream_logs {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let selected_index = app.streams_table_state.selected().unwrap_or(0);
    let stream_position = selected_index + 1; // 1-indexed
    let total_streams = stats.len();

    streams::render_streams_panel(
        stats,
        table_area,
        frame,
        &mut app.streams_table_state,
        app.show_stream_logs,
        app.streams_focus,
        stream_position,
        total_streams,
    );

    // Render logs panel if visible
    if let Some(logs_area) = logs_area {
        let stream_label = app
            .streams_table_state
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

        if let Some(ref cached_logs) = app.stream_logs {
            let has_missing_log = cached_logs
                .logs
                .logs
                .iter()
                .any(|entry| entry.message.is_none());
            let display_label = if has_missing_log {
                format!("{} (missing \"log = true\")", stream_label)
            } else {
                stream_label
            };
            stream_logs::render_logs_panel(
                cached_logs,
                &display_label,
                logs_area,
                frame,
                &mut app.stream_logs_table_state,
                app.streams_focus == StreamsFocus::Logs,
                app.streams.current_elapsed_ns,
            );
        } else {
            let message = if app.paused {
                "(refresh paused)"
            } else if app.error_message.is_some() {
                "(cannot fetch new data)"
            } else {
                "(no data)"
            };
            stream_logs::render_logs_placeholder(&stream_label, message, logs_area, frame);
        }
    }

    if app.streams_focus == StreamsFocus::Inspect {
        if let Some(ref inspected_log) = app.inspected_stream_log {
            stream_inspect::render_inspect_popup(inspected_log, area, frame);
        }
    }
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn render_tabs(frame: &mut Frame, area: ratatui::layout::Rect, selected_tab: SelectedTab) {
    let create_tab_line = |tab: SelectedTab| {
        let name = if tab == selected_tab {
            format!(" {}*", tab.name())
        } else {
            format!(" {} ", tab.name())
        };
        Line::from(vec![
            Span::styled(
                format!("[{}]", tab.number()),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(name, Style::default().fg(Color::Gray)),
        ])
    };

    let titles = vec![
        create_tab_line(SelectedTab::Timing),
        create_tab_line(SelectedTab::Memory),
        create_tab_line(SelectedTab::Channels),
        create_tab_line(SelectedTab::Streams),
    ];

    let selected_index = (selected_tab.number() - 1) as usize;

    let tabs = Tabs::new(titles)
        .select(selected_index)
        .divider(" | ")
        .style(Style::default())
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}
