//! Keyboard input handling

use super::{App, ChannelsFocus, FunctionsFocus, SelectedTab, StreamsFocus};
use crossterm::event::KeyCode;

#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl App {
    pub(crate) fn handle_key_event(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.exit(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.toggle_pause(),
            KeyCode::Char('1') => {
                self.switch_to_tab(SelectedTab::Timing);
                self.refresh_data();
            }
            KeyCode::Char('2') => {
                self.switch_to_tab(SelectedTab::Memory);
                self.refresh_data();
            }
            KeyCode::Char('3') => {
                self.switch_to_tab(SelectedTab::Channels);
                self.refresh_data();
            }
            KeyCode::Char('4') => {
                self.switch_to_tab(SelectedTab::Streams);
                self.refresh_data();
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.channels_focus {
                        ChannelsFocus::Inspect => self.close_inspect_and_refocus_channels(),
                        ChannelsFocus::Logs => self.hide_logs(),
                        ChannelsFocus::Channels => self.toggle_logs(),
                    }
                } else if self.selected_tab == SelectedTab::Streams {
                    match self.streams_focus {
                        StreamsFocus::Inspect => self.close_stream_inspect_and_refocus_streams(),
                        StreamsFocus::Logs => self.hide_stream_logs(),
                        StreamsFocus::Streams => self.toggle_stream_logs(),
                    }
                } else {
                    self.toggle_function_logs();
                    self.fetch_function_logs_if_open(self.metrics_port);
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                if self.selected_tab == SelectedTab::Channels {
                    if self.channels_focus == ChannelsFocus::Inspect {
                        self.close_inspect_only();
                    } else {
                        self.focus_channels();
                    }
                } else if self.selected_tab == SelectedTab::Streams {
                    if self.streams_focus == StreamsFocus::Inspect {
                        self.close_stream_inspect_only();
                    } else {
                        self.focus_streams();
                    }
                } else if self.selected_tab.is_functions_tab() {
                    self.focus_functions();
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.selected_tab == SelectedTab::Channels {
                    self.focus_logs();
                } else if self.selected_tab == SelectedTab::Streams {
                    self.focus_stream_logs();
                } else if self.selected_tab.is_functions_tab() {
                    self.focus_function_logs();
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                if self.selected_tab == SelectedTab::Channels {
                    self.toggle_inspect();
                } else if self.selected_tab == SelectedTab::Streams {
                    self.toggle_stream_inspect();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.channels_focus {
                        ChannelsFocus::Channels => self.select_next_channel(),
                        ChannelsFocus::Logs | ChannelsFocus::Inspect => self.select_next_log(),
                    }
                } else if self.selected_tab == SelectedTab::Streams {
                    match self.streams_focus {
                        StreamsFocus::Streams => self.select_next_stream(),
                        StreamsFocus::Logs | StreamsFocus::Inspect => self.select_next_stream_log(),
                    }
                } else if self.selected_tab.is_functions_tab() {
                    match self.functions_focus {
                        FunctionsFocus::Functions => {
                            self.next_function();
                            self.update_and_fetch_function_logs(self.metrics_port);
                        }
                        FunctionsFocus::Logs => self.select_next_function_log(),
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.channels_focus {
                        ChannelsFocus::Channels => self.select_previous_channel(),
                        ChannelsFocus::Logs | ChannelsFocus::Inspect => self.select_previous_log(),
                    }
                } else if self.selected_tab == SelectedTab::Streams {
                    match self.streams_focus {
                        StreamsFocus::Streams => self.select_previous_stream(),
                        StreamsFocus::Logs | StreamsFocus::Inspect => {
                            self.select_previous_stream_log()
                        }
                    }
                } else if self.selected_tab.is_functions_tab() {
                    match self.functions_focus {
                        FunctionsFocus::Functions => {
                            self.previous_function();
                            self.update_and_fetch_function_logs(self.metrics_port);
                        }
                        FunctionsFocus::Logs => self.select_previous_function_log(),
                    }
                }
            }
            _ => {}
        }
    }
}
