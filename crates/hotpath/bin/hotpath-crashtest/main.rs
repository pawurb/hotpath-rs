mod cmd;

use cmd::run;
use eyre::Result;

fn main() -> Result<()> {
    run::run()
}
