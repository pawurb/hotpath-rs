use super::super::app::{App, SelectedTab};
use super::{bottom_bar, channels, functions, samples, top_bar};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::Tabs,
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

    top_bar::render_status_bar(
        frame,
        main_chunks[1],
        app.paused,
        &app.error_message,
        &app.last_successful_fetch,
        app.last_refresh,
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
            channels::render_channels_table(frame, app, main_chunks[2]);
        }
    }

    bottom_bar::render_help_bar(frame, main_chunks[3]);
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
