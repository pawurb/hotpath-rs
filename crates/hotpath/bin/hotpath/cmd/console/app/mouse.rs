//! Mouse event handling for the TUI

use crate::cmd::console::app::{App, SubTabHit};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

#[hotpath::measure_all]
impl App {
    pub(crate) fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let pos = Position::new(mouse.column, mouse.row);

        if let Some(tab) = self
            .tab_hit_areas
            .iter()
            .find(|(rect, _)| rect.contains(pos))
            .map(|&(_, tab)| tab)
        {
            if tab != self.selected_tab {
                self.switch_to_tab(tab);
            }
            return;
        }

        if let Some(hit) = self
            .sub_tab_hit_areas
            .iter()
            .find(|(rect, _)| rect.contains(pos))
            .map(|&(_, hit)| hit)
        {
            match hit {
                SubTabHit::Functions(tab) => self.set_functions_sub_tab(tab),
                SubTabHit::DataFlow(tab) => self.set_data_flow_sub_tab(tab),
                SubTabHit::Io(tab) => self.set_io_sub_tab(tab),
            }
        }
    }
}
