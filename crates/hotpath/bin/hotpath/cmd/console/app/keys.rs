//! Keyboard event handling for the TUI

use crate::cmd::console::app::{App, DataFlowFocus, DebugFocus, FunctionsFocus, SelectedTab};
use crossterm::event::KeyCode;
use std::time::{Duration, Instant};

#[hotpath::measure_all]
impl App {
    pub(crate) fn handle_key_event(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.exit(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.toggle_pause(),
            KeyCode::Char('1') => self.switch_to_tab(SelectedTab::Timing),
            KeyCode::Char('2') => self.switch_to_tab(SelectedTab::Memory),
            KeyCode::Char('3') => self.switch_to_tab(SelectedTab::DataFlow),
            KeyCode::Char('4') => self.switch_to_tab(SelectedTab::Threads),
            KeyCode::Char('5') => self.switch_to_tab(SelectedTab::Debug),
            KeyCode::Char('6') => self.switch_to_tab(SelectedTab::Runtime),
            _ => self.handle_tab_specific_key(key_code),
        }
    }

    fn handle_g_key(&mut self) -> bool {
        if let Some(last_g) = self.pending_g {
            if last_g.elapsed() < Duration::from_millis(500) {
                self.pending_g = None;
                return true;
            }
        }
        self.pending_g = Some(Instant::now());
        false
    }

    fn handle_tab_specific_key(&mut self, key_code: KeyCode) {
        match self.selected_tab {
            SelectedTab::Timing | SelectedTab::Memory => {
                self.handle_functions_key(key_code);
            }
            SelectedTab::DataFlow => {
                self.handle_data_flow_key(key_code);
            }
            SelectedTab::Threads => {
                self.handle_threads_key(key_code);
            }
            SelectedTab::Debug => {
                self.handle_debug_key(key_code);
            }
            SelectedTab::Runtime => {
                self.handle_runtime_key(key_code);
            }
        }
    }

    fn handle_functions_key(&mut self, key_code: KeyCode) {
        match self.functions_focus {
            FunctionsFocus::Functions => match key_code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.next_function();
                    self.update_and_request_function_logs();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.previous_function();
                    self.update_and_request_function_logs();
                }
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_function();
                    self.update_and_request_function_logs();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_function();
                        self.update_and_request_function_logs();
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') => self.toggle_function_logs(),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.focus_function_logs(),
                _ => {}
            },
            FunctionsFocus::Logs => match key_code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_function_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_function_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_function_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_function_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_function_inspect()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => self.focus_functions(),
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.toggle_function_logs()
                }
                _ => {}
            },
            FunctionsFocus::Inspect => match key_code {
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.close_function_inspect_and_refocus_functions()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                    self.close_function_inspect_only()
                }
                KeyCode::Down | KeyCode::Char('j') => self.select_next_function_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_function_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_function_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_function_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_function_inspect()
                }
                _ => {}
            },
        }
    }

    fn handle_data_flow_key(&mut self, key_code: KeyCode) {
        match self.data_flow_focus {
            DataFlowFocus::List => match key_code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_data_flow(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_data_flow(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_data_flow();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_data_flow();
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') => self.toggle_data_flow_logs(),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.focus_data_flow_logs(),
                _ => {}
            },
            DataFlowFocus::Logs => match key_code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_data_flow_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_data_flow_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_data_flow_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_data_flow_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_data_flow_inspect()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                    self.focus_data_flow_list()
                }
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.toggle_data_flow_logs()
                }
                _ => {}
            },
            DataFlowFocus::Inspect => match key_code {
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.close_data_flow_inspect_and_refocus()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                    self.close_data_flow_inspect_only()
                }
                KeyCode::Down | KeyCode::Char('j') => self.select_next_data_flow_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_data_flow_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_data_flow_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_data_flow_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_data_flow_inspect()
                }
                _ => {}
            },
        }
    }

    fn handle_threads_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Down | KeyCode::Char('j') => self.select_next_thread(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_thread(),
            KeyCode::Char('G') => {
                self.pending_g = None;
                self.last_thread();
            }
            KeyCode::Char('g') => {
                if self.handle_g_key() {
                    self.first_thread();
                }
            }
            _ => {}
        }
    }

    fn handle_runtime_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Down | KeyCode::Char('j') => self.select_next_runtime_worker(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_runtime_worker(),
            KeyCode::Char('G') => {
                self.pending_g = None;
                self.last_runtime_worker();
            }
            KeyCode::Char('g') => {
                if self.handle_g_key() {
                    self.first_runtime_worker();
                }
            }
            _ => {}
        }
    }

    fn handle_debug_key(&mut self, key_code: KeyCode) {
        match self.debug_focus {
            DebugFocus::Debug => match key_code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_debug(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_debug(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_debug();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_debug();
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') => self.toggle_debug_logs(),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.focus_debug_logs(),
                _ => {}
            },
            DebugFocus::Logs => match key_code {
                KeyCode::Down | KeyCode::Char('j') => self.select_next_debug_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_debug_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_debug_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_debug_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_debug_inspect()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => self.focus_debug(),
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => self.toggle_debug_logs(),
                _ => {}
            },
            DebugFocus::Inspect => match key_code {
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.close_debug_inspect_and_refocus_debug()
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
                    self.close_debug_inspect_only()
                }
                KeyCode::Down | KeyCode::Char('j') => self.select_next_debug_log(),
                KeyCode::Up | KeyCode::Char('k') => self.select_previous_debug_log(),
                KeyCode::Char('G') => {
                    self.pending_g = None;
                    self.last_debug_log();
                }
                KeyCode::Char('g') => {
                    if self.handle_g_key() {
                        self.first_debug_log();
                    }
                }
                KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.toggle_debug_inspect()
                }
                _ => {}
            },
        }
    }
}
