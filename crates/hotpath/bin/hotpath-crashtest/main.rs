mod cmd;
mod scenarios;

use cmd::run;
use eyre::Result;

#[hotpath::main]
fn main() -> Result<()> {
    run::run()
}
