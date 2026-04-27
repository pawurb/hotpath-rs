use crate::cmd::console::app::{
    App, DataFlowFocus, DataFlowLogs, DataFlowSubTab, DebugFocus, FunctionsFocus, FunctionsSubTab,
    SelectedTab,
};
use crate::cmd::console::views::data_flow::{inspect as data_flow_inspect, logs as data_flow_logs};
use crate::cmd::console::views::debug::{inspect as debug_inspect, logs as debug_logs};
use crate::cmd::console::views::functions_memory::{
    inspect as memory_inspect, logs as memory_logs,
};
use crate::cmd::console::views::functions_timing::{
    inspect as timing_inspect, logs as timing_logs,
};
use crate::cmd::console::views::{
    bottom_bar, data_flow, debug, functions_memory, functions_timing, runtime, threads, top_bar,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Paragraph, Tabs},
    Frame,
};

#[hotpath::measure]
pub(crate) fn render_ui(frame: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs
            Constraint::Length(1), // Subtabs (Functions only)
            Constraint::Length(3), // Status bar
            Constraint::Min(0),    // Main content area
            Constraint::Length(3), // Help bar
        ])
        .split(frame.area());

    let has_data = match app.selected_tab {
        SelectedTab::Functions => match app.functions_sub_tab {
            FunctionsSubTab::Timing => !app.timing_functions.data.is_empty(),
            FunctionsSubTab::Memory => !app.memory_functions.data.is_empty(),
        },
        SelectedTab::DataFlow => app.data_flow_entries_len() > 0,
        SelectedTab::Threads => !app.threads.data.is_empty(),
        SelectedTab::Debug => !app.debug_stats.is_empty(),
        SelectedTab::Runtime => app.tokio_runtime.is_some(),
    };

    render_tabs(frame, main_chunks[0], app.selected_tab);

    if app.selected_tab == SelectedTab::Functions {
        render_functions_subtabs(frame, main_chunks[1], app.functions_sub_tab);
    } else if app.selected_tab == SelectedTab::DataFlow {
        render_data_flow_subtabs(frame, main_chunks[1], app.data_flow_sub_tab);
    }

    top_bar::render_status_bar(
        frame,
        main_chunks[2],
        app.paused,
        app.last_successful_fetch,
        app.error_message.is_some(),
        has_data,
        app.program_uptime.as_deref(),
    );

    match app.selected_tab {
        SelectedTab::Functions => {
            render_functions_view(frame, app, main_chunks[3]);
        }
        SelectedTab::DataFlow => {
            render_data_flow_view(frame, app, main_chunks[3]);
        }
        SelectedTab::Threads => {
            render_threads_view(frame, app, main_chunks[3]);
        }
        SelectedTab::Debug => {
            render_debug_view(frame, app, main_chunks[3]);
        }
        SelectedTab::Runtime => {
            render_runtime_view(frame, app, main_chunks[3]);
        }
    }

    bottom_bar::render_help_bar(
        frame,
        main_chunks[4],
        app.selected_tab,
        app.data_flow_focus,
        app.functions_focus,
        app.debug_focus,
    );
}

#[hotpath::measure]
fn render_functions_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let content_area = area;

    match app.functions_sub_tab {
        FunctionsSubTab::Timing => {
            if app.show_function_logs {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(content_area);

                functions_timing::render_functions_table(frame, app, content_chunks[0]);
                timing_logs::render_function_logs_panel(
                    app.current_timing_logs.as_ref(),
                    app.selected_function_name().as_deref(),
                    content_chunks[1],
                    frame,
                    &mut app.function_logs_table_state,
                    app.functions_focus == FunctionsFocus::Logs,
                );

                if app.functions_focus == FunctionsFocus::Inspect {
                    if let Some(ref inspected_log) = app.inspected_function_log {
                        timing_inspect::render_inspect_popup(
                            inspected_log,
                            content_area,
                            frame,
                            app.timing_functions.total_elapsed_ns,
                        );
                    }
                }
            } else {
                functions_timing::render_functions_table(frame, app, content_area);
            }
        }
        FunctionsSubTab::Memory => {
            if app.show_function_logs {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(content_area);

                functions_memory::render_functions_table(frame, app, content_chunks[0]);
                memory_logs::render_function_logs_panel(
                    app.current_alloc_logs.as_ref(),
                    app.selected_function_name().as_deref(),
                    content_chunks[1],
                    frame,
                    &mut app.function_logs_table_state,
                    app.functions_focus == FunctionsFocus::Logs,
                );

                if app.functions_focus == FunctionsFocus::Inspect {
                    if let Some(ref inspected_log) = app.inspected_function_log {
                        memory_inspect::render_inspect_popup(
                            inspected_log,
                            content_area,
                            frame,
                            app.memory_functions.total_elapsed_ns,
                        );
                    }
                }
            } else {
                functions_memory::render_functions_table(frame, app, content_area);
            }
        }
    }
}

#[hotpath::measure]
fn render_functions_subtabs(frame: &mut Frame, area: Rect, sub_tab: FunctionsSubTab) {
    let label = |tab: FunctionsSubTab| {
        if tab == sub_tab {
            Span::styled(
                format!(" {}*", tab.name()),
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                format!(" {} ", tab.name()),
                Style::default().fg(Color::Gray),
            )
        }
    };

    let line = Line::from(vec![
        Span::styled(
            " [1]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        label(FunctionsSubTab::Timing),
        Span::raw("|"),
        label(FunctionsSubTab::Memory),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

#[hotpath::measure]
fn render_data_flow_subtabs(frame: &mut Frame, area: Rect, sub_tab: DataFlowSubTab) {
    let label = |tab: DataFlowSubTab| {
        if tab == sub_tab {
            Span::styled(
                format!(" {}*", tab.name()),
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                format!(" {} ", tab.name()),
                Style::default().fg(Color::Gray),
            )
        }
    };

    let line = Line::from(vec![
        Span::styled(
            " [2]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        label(DataFlowSubTab::Channels),
        Span::raw("|"),
        label(DataFlowSubTab::Streams),
        Span::raw("|"),
        label(DataFlowSubTab::Futures),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

#[hotpath::measure]
fn render_data_flow_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let total = app.data_flow_entries_len();

    if let Some(ref error_msg) = app.error_message {
        if total == 0 {
            let error_text = vec![
                Line::from(""),
                Line::from("Error").red().bold().centered(),
                Line::from(""),
                Line::from(error_msg.as_str()).red().centered(),
                Line::from(""),
                Line::from(format!(
                    "Make sure the metrics server is running on {}",
                    app.metrics_host
                ))
                .yellow()
                .centered(),
            ];

            let block = Block::bordered().border_set(border::THICK);
            frame.render_widget(Paragraph::new(error_text).block(block), area);
            return;
        }
    }

    if total == 0 {
        let empty_lines = match app.data_flow_sub_tab {
            DataFlowSubTab::Channels => vec![
                Line::from(""),
                Line::from("No channels found").yellow().centered(),
                Line::from(""),
                Line::from("Use the channel! macro to instrument channels").centered(),
            ],
            DataFlowSubTab::Streams => vec![
                Line::from(""),
                Line::from("No streams found").yellow().centered(),
                Line::from(""),
                Line::from("Use the stream! macro to instrument streams").centered(),
            ],
            DataFlowSubTab::Futures => vec![
                Line::from(""),
                Line::from("No futures found").yellow().centered(),
                Line::from(""),
                Line::from("Use the future! macro to instrument futures").centered(),
            ],
        };

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_lines).block(block), area);
        return;
    }

    let (table_area, logs_area) = if app.show_data_flow_logs {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let selected_index = app.data_flow_table_state().selected().unwrap_or(0);
    let position = selected_index + 1;

    let label = match app.data_flow_sub_tab {
        DataFlowSubTab::Channels => app
            .channels_table_state
            .selected()
            .and_then(|i| app.channels.data.get(i))
            .map(|e| {
                if e.label.is_empty() {
                    e.id.to_string()
                } else {
                    e.label.clone()
                }
            })
            .unwrap_or_else(|| "Unknown".to_string()),
        DataFlowSubTab::Streams => app
            .streams_table_state
            .selected()
            .and_then(|i| app.streams.data.get(i))
            .map(|e| {
                if e.label.is_empty() {
                    e.id.to_string()
                } else {
                    e.label.clone()
                }
            })
            .unwrap_or_else(|| "Unknown".to_string()),
        DataFlowSubTab::Futures => app
            .futures_table_state
            .selected()
            .and_then(|i| app.futures.data.get(i))
            .map(|e| {
                if e.label.is_empty() {
                    e.id.to_string()
                } else {
                    e.label.clone()
                }
            })
            .unwrap_or_else(|| "Unknown".to_string()),
    };

    match app.data_flow_sub_tab {
        DataFlowSubTab::Channels => data_flow::render_channels_panel(
            &app.channels.data,
            table_area,
            frame,
            &mut app.channels_table_state,
            app.show_data_flow_logs,
            app.data_flow_focus,
            position,
            total,
        ),
        DataFlowSubTab::Streams => data_flow::render_streams_panel(
            &app.streams.data,
            table_area,
            frame,
            &mut app.streams_table_state,
            app.show_data_flow_logs,
            app.data_flow_focus,
            position,
            total,
        ),
        DataFlowSubTab::Futures => data_flow::render_futures_panel(
            &app.futures.data,
            table_area,
            frame,
            &mut app.futures_table_state,
            app.show_data_flow_logs,
            app.data_flow_focus,
            position,
            total,
        ),
    };

    if let Some(logs_area) = logs_area {
        if let Some(ref logs) = app.data_flow_logs {
            let has_missing_log = match logs {
                DataFlowLogs::Channel(l) => l.sent_logs.iter().any(|e| e.message.is_none()),
                DataFlowLogs::Stream(l) => l.logs.iter().any(|e| e.message.is_none()),
                DataFlowLogs::Future(_) => false,
            };
            data_flow_logs::render_logs_panel(
                logs,
                app.data_flow_sub_tab,
                &label,
                has_missing_log,
                logs_area,
                frame,
                &mut app.data_flow_logs_table_state,
                app.data_flow_focus == DataFlowFocus::Logs,
            );
        } else {
            let message = if app.paused {
                "(refresh paused)"
            } else if app.error_message.is_some() {
                "(cannot fetch new data)"
            } else {
                "(no data)"
            };
            data_flow_logs::render_logs_placeholder(&label, message, logs_area, frame);
        }
    }

    if app.data_flow_focus == DataFlowFocus::Inspect {
        if let Some(ref inspected) = app.inspected_data_flow_log {
            data_flow_inspect::render_inspect_popup(inspected, &label, area, frame);
        }
    }
}

#[hotpath::measure]
fn render_threads_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let thread_list = &app.threads.data;

    if let Some(ref error_msg) = app.error_message {
        if thread_list.is_empty() {
            let error_text = vec![
                Line::from(""),
                Line::from("Error").red().bold().centered(),
                Line::from(""),
                Line::from(error_msg.as_str()).red().centered(),
                Line::from(""),
                Line::from(format!(
                    "Make sure the metrics server is running on {}",
                    app.metrics_host
                ))
                .yellow()
                .centered(),
            ];

            let block = Block::bordered().border_set(border::THICK);
            frame.render_widget(Paragraph::new(error_text).block(block), area);
            return;
        }
    }

    if thread_list.is_empty() {
        let empty_text = vec![
            Line::from(""),
            Line::from("No thread statistics found").yellow().centered(),
            Line::from(""),
            Line::from("Make sure thread monitoring is enabled and the server is running")
                .centered(),
        ];

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_text).block(block), area);
        return;
    }

    let selected_index = app.threads_table_state.selected().unwrap_or(0);
    let thread_position = selected_index + 1;
    let total_threads = thread_list.len();

    threads::render_threads_panel(
        thread_list,
        area,
        frame,
        &mut app.threads_table_state,
        thread_position,
        total_threads,
        app.threads.rss_bytes.as_deref(),
        app.threads.total_alloc_bytes.as_deref(),
        app.threads.total_dealloc_bytes.as_deref(),
        app.threads.alloc_dealloc_diff.as_deref(),
    );
}

#[hotpath::measure]
fn render_debug_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let stats = &app.debug_stats;

    if let Some(ref error_msg) = app.error_message {
        if stats.is_empty() {
            let error_text = vec![
                Line::from(""),
                Line::from("Error").red().bold().centered(),
                Line::from(""),
                Line::from(error_msg.as_str()).red().centered(),
                Line::from(""),
                Line::from(format!(
                    "Make sure the metrics server is running on {}",
                    app.metrics_host
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
            Line::from("No debug logs found").yellow().centered(),
            Line::from(""),
            Line::from("Use hotpath::dbg!, hotpath::gauge!, or hotpath::val! macros.").centered(),
        ];

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_text).block(block), area);
        return;
    }

    let (table_area, logs_area) = if app.show_debug_logs {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let selected_index = app.debug_table_state.selected().unwrap_or(0);
    let debug_position = selected_index + 1;
    let total_debug = stats.len();

    debug::render_debug_panel(
        stats,
        table_area,
        frame,
        &mut app.debug_table_state,
        app.show_debug_logs,
        app.debug_focus,
        debug_position,
        total_debug,
    );

    if let Some(logs_area) = logs_area {
        let source_label = app
            .debug_table_state
            .selected()
            .and_then(|i| stats.get(i))
            .map(|stat| stat.source_display.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        if let Some(ref cached_logs) = app.debug_logs {
            debug_logs::render_debug_logs_panel(
                cached_logs,
                &source_label,
                logs_area,
                frame,
                &mut app.debug_logs_table_state,
                app.debug_focus == DebugFocus::Logs,
            );
        } else {
            let message = if app.paused {
                "(refresh paused)"
            } else if app.error_message.is_some() {
                "(cannot fetch new data)"
            } else {
                "(no data)"
            };
            debug_logs::render_debug_logs_placeholder(&source_label, message, logs_area, frame);
        }
    }

    if app.debug_focus == DebugFocus::Inspect {
        if let Some(ref inspected_log) = app.inspected_debug_log {
            debug_inspect::render_debug_inspect_popup(inspected_log, area, frame);
        }
    }
}

#[hotpath::measure]
fn render_runtime_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let snap = app.tokio_runtime.as_ref();

    if let Some(ref error_msg) = app.error_message {
        if snap.is_none() {
            let error_text = vec![
                Line::from(""),
                Line::from("Error").red().bold().centered(),
                Line::from(""),
                Line::from(error_msg.as_str()).red().centered(),
            ];

            let block = Block::bordered().border_set(border::THICK);
            frame.render_widget(Paragraph::new(error_text).block(block), area);
            return;
        }
    }

    let Some(snap) = snap else {
        let empty_text = vec![
            Line::from(""),
            Line::from("No Tokio runtime metrics available")
                .yellow()
                .centered(),
            Line::from(""),
            Line::from("Use hotpath::tokio_runtime!() in your application").centered(),
        ];

        let block = Block::bordered().border_set(border::THICK);
        frame.render_widget(Paragraph::new(empty_text).block(block), area);
        return;
    };

    let selected_index = app.runtime_table_state.selected().unwrap_or(0);
    let worker_position = selected_index + 1;
    let total_workers = snap.workers.len();

    runtime::render_runtime_panel(
        snap,
        area,
        frame,
        &mut app.runtime_table_state,
        worker_position,
        total_workers,
    );
}

#[hotpath::measure]
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
        create_tab_line(SelectedTab::Functions),
        create_tab_line(SelectedTab::DataFlow),
        create_tab_line(SelectedTab::Threads),
        create_tab_line(SelectedTab::Debug),
        create_tab_line(SelectedTab::Runtime),
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
