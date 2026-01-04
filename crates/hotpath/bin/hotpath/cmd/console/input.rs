//! Dedicated thread for reading keyboard events

use crossbeam_channel::Sender;
use crossterm::event::{self, Event, KeyEventKind};

use super::events::AppEvent;

pub(crate) fn spawn_input_reader(event_tx: Sender<AppEvent>) {
    std::thread::spawn(move || loop {
        if let Ok(evt) = event::read() {
            let app_event = match evt {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    Some(AppEvent::Key(key_event.code))
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
