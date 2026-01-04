mod app;
mod constants;
#[cfg(feature = "hotpath")]
pub mod demo;
mod events;
mod http_worker;
mod input;
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

#[hotpath::measure_all]
impl ConsoleArgs {
    pub fn run(&self) -> Result<()> {
        // Demo auto-instrumenting streams, channels and futures
        // is only available when the hotpath feature is enabled
        #[cfg(feature = "hotpath")]
        demo::init();

        let mut app = App::new(self.metrics_port, self.refresh_interval);

        // Use modern ratatui initialization
        let mut terminal = ratatui::init();

        let app_result = app.run(&mut terminal);

        // Use modern ratatui restoration
        ratatui::restore();

        app_result.map_err(|e| eyre::eyre!("TUI error: {}", e))
    }
}
