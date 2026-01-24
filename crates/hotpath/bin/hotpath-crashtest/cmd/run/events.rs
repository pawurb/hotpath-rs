use crossterm::event::KeyCode;

pub enum AppEvent {
    Key(KeyCode),
}
