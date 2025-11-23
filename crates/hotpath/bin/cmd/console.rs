mod app;
mod constants;
mod http;
mod views;
mod widgets;

use app::App;
use clap::Parser;
use eyre::Result;

#[derive(Debug, Parser)]
pub struct ConsoleArgs {
    #[arg(
        long,
        default_value_t = 6770,
        help = "Port where the metrics HTTP server is running"
    )]
    pub metrics_port: u16,

    #[arg(long, default_value_t = 500, help = "Refresh interval in milliseconds")]
    pub refresh_interval: u64,
}

#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl ConsoleArgs {
    pub fn run(&self) -> Result<()> {
        let mut app = App::new(self.metrics_port);

        // Use modern ratatui initialization
        let mut terminal = ratatui::init();

        let app_result = app.run(&mut terminal, self.refresh_interval);

        // Use modern ratatui restoration
        ratatui::restore();

        app_result.map_err(|e| eyre::eyre!("TUI error: {}", e))
    }
}
