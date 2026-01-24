use crate::cmd::run::events::AppEvent;
use crossbeam_channel::Sender;
use crossterm::event::{self, Event, KeyEventKind};

pub fn spawn_input_reader(tx: Sender<AppEvent>) {
    std::thread::spawn(move || loop {
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind == KeyEventKind::Press && tx.send(AppEvent::Key(key.code)).is_err() {
                break;
            }
        }
    });
}
