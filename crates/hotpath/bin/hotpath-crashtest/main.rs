mod cmd;
mod scenarios;

use cmd::run;
use eyre::Result;

fn main() -> Result<()> {
    run::run()
}
