mod cmd;

#[cfg(any(feature = "tui", feature = "cloud"))]
use clap::{Parser, Subcommand};
#[cfg(feature = "cloud")]
use cmd::cloud::CloudArgs;
#[cfg(feature = "tui")]
use cmd::console::ConsoleArgs;
#[cfg(any(feature = "tui", feature = "cloud"))]
use std::process::ExitCode;

#[cfg(any(feature = "tui", feature = "cloud"))]
#[derive(Parser, Debug)]
pub struct InitCliArgs {
    #[arg(long, help = "AI agent to launch: claude, codex or opencode")]
    pub agent: String,
}

/// Placeholder for `hotpath cloud` in a binary built without the `cloud`
/// feature: keeps the command visible in `--help` and turns an attempt to
/// run it into a hint instead of clap's "unrecognized subcommand".
#[cfg(all(any(feature = "tui", feature = "cloud"), not(feature = "cloud")))]
#[derive(Parser, Debug)]
pub struct CloudUnavailableArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    pub rest: Vec<String>,
}

#[cfg(any(feature = "tui", feature = "cloud"))]
#[derive(Subcommand, Debug)]
pub enum HPSubcommand {
    #[cfg(feature = "tui")]
    #[command(about = "Launch TUI console to monitor profiling metrics in real-time")]
    Console(ConsoleArgs),
    #[command(about = "Configure hotpath in the current repo via an AI agent session")]
    Init(InitCliArgs),
    #[cfg(feature = "cloud")]
    #[command(about = "Query hotpath.rs: authentication status and cloud reports")]
    Cloud(CloudArgs),
    #[cfg(not(feature = "cloud"))]
    #[command(about = "Query hotpath.rs (requires building with the 'cloud' feature)")]
    Cloud(CloudUnavailableArgs),
}

#[cfg(any(feature = "tui", feature = "cloud"))]
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

    #[cfg(feature = "tui")]
    #[command(flatten)]
    pub console_args: ConsoleArgs,
}

#[cfg(not(feature = "cloud"))]
pub const CLOUD_FEATURE_HINT: &str =
    "The 'cloud' command requires building with the 'cloud' feature: cargo install hotpath --features cloud";

#[cfg(any(feature = "tui", feature = "cloud"))]
#[hotpath::main(limit = 10)]
fn main() -> eyre::Result<ExitCode> {
    let root_args = HPArgs::parse();

    match root_args.cmd {
        #[cfg(feature = "tui")]
        Some(HPSubcommand::Console(args)) => args.run()?,
        Some(HPSubcommand::Init(args)) => {
            let agent = cmd::init::Agent::from_arg(&args.agent).map_err(|e| eyre::eyre!(e))?;
            cmd::init::run(agent).map_err(|e| eyre::eyre!(e))?;
        }
        #[cfg(feature = "cloud")]
        Some(HPSubcommand::Cloud(args)) => return Ok(args.run()),
        #[cfg(not(feature = "cloud"))]
        Some(HPSubcommand::Cloud(_)) => {
            eprintln!("{CLOUD_FEATURE_HINT}");
            return Ok(ExitCode::FAILURE);
        }
        #[cfg(feature = "tui")]
        None => root_args.console_args.run()?,
        #[cfg(not(feature = "tui"))]
        None => {
            use clap::CommandFactory;
            HPArgs::command().print_help()?;
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(not(any(feature = "tui", feature = "cloud")))]
fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("init") => {
            let flag = args.next();
            let agent_arg = match (flag.as_deref(), args.next()) {
                (Some("--agent"), Some(agent)) => Ok(agent),
                (Some(flag), None) if flag.starts_with("--agent=") => {
                    Ok(flag["--agent=".len()..].to_string())
                }
                _ => Err("Usage: hotpath init --agent <claude|codex|opencode>".to_string()),
            };
            let result = agent_arg
                .and_then(|agent| cmd::init::Agent::from_arg(&agent))
                .and_then(cmd::init::run);
            match result {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Some("cloud") => {
            eprintln!("{CLOUD_FEATURE_HINT}");
            std::process::ExitCode::FAILURE
        }
        _ => {
            eprintln!(
                "hotpath CLI

Usage: hotpath <COMMAND>

Commands:
  init --agent <claude|codex|opencode>  Configure hotpath in the current repo via an AI agent session

The 'console' command requires building with the 'tui' feature.
The 'cloud' command requires building with the 'cloud' feature."
            );
            std::process::ExitCode::FAILURE
        }
    }
}
