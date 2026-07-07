//! Dedicated thread for reading keyboard and mouse events

use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use hotpath::wrap::crossbeam_channel::Sender;

use super::events::AppEvent;

pub(crate) fn spawn_input_reader(event_tx: Sender<AppEvent>) {
    std::thread::spawn(move || loop {
        if let Ok(evt) = event::read() {
            let app_event = match evt {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    Some(AppEvent::Key(key_event.code))
                }
                Event::Mouse(mouse_event)
                    if mouse_event.kind == MouseEventKind::Down(MouseButton::Left) =>
                {
                    Some(AppEvent::Mouse(mouse_event))
                }
                _ => None,
            };

            if let Some(app_event) = app_event {
                if event_tx.send(app_event).is_err() {
                    break;
                }
            }
        }
    });
}
