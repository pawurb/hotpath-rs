mod app;
mod events;
mod input;

use app::App;
use eyre::Result;

pub fn run() -> Result<()> {
    let mut app = App::new();
    let mut terminal = ratatui::init();

    let result = app.run(&mut terminal);

    ratatui::restore();

    result.map_err(|e| eyre::eyre!("TUI error: {}", e))
}
