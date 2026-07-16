mod cmd;

#[cfg(feature = "tui")]
use clap::{Parser, Subcommand};
#[cfg(feature = "tui")]
use cmd::console::ConsoleArgs;

#[cfg(feature = "tui")]
#[derive(Parser, Debug)]
pub struct InitCliArgs {
    #[arg(default_value = "claude", help = "AI agent to launch: claude or codex")]
    pub agent: String,
}

#[cfg(feature = "tui")]
#[derive(Subcommand, Debug)]
pub enum HPSubcommand {
    #[command(about = "Launch TUI console to monitor profiling metrics in real-time")]
    Console(ConsoleArgs),
    #[command(about = "Configure hotpath in the current repo via an AI agent session")]
    Init(InitCliArgs),
}

#[cfg(feature = "tui")]
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "hotpath CLI: automatically profile Rust programs on each Pull Request

https://github.com/pawurb/hotpath-rs",
    args_conflicts_with_subcommands = true
)]
pub struct HPArgs {
    #[command(subcommand)]
    pub cmd: Option<HPSubcommand>,

    #[command(flatten)]
    pub console_args: ConsoleArgs,
}

#[cfg(feature = "tui")]
#[hotpath::main(limit = 10)]
fn main() -> eyre::Result<()> {
    let root_args = HPArgs::parse();

    match root_args.cmd {
        Some(HPSubcommand::Console(args)) => args.run()?,
        Some(HPSubcommand::Init(args)) => {
            let agent =
                cmd::init::Agent::from_arg(Some(&args.agent)).map_err(|e| eyre::eyre!(e))?;
            cmd::init::run(agent).map_err(|e| eyre::eyre!(e))?;
        }
        None => root_args.console_args.run()?,
    }

    Ok(())
}

#[cfg(not(feature = "tui"))]
fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("init") => {
            let agent_arg = args.next();
            let result = cmd::init::Agent::from_arg(agent_arg.as_deref()).and_then(cmd::init::run);
            match result {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "hotpath CLI

Usage: hotpath <COMMAND>

Commands:
  init [claude|codex]  Configure hotpath in the current repo via an AI agent session

The 'console' command requires building with the 'tui' feature."
            );
            std::process::ExitCode::FAILURE
        }
    }
}
