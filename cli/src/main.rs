mod games;
mod hotseat;
mod paths;
mod play;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Opens the terminal interface when omitted
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the terminal interface: pick a game and play hotseat on this machine
    Play(play::Args),
    /// Play a match on one machine, driven line by line from stdin. No TUI, no network.
    Hotseat(hotseat::Args),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _log_guard = init_logging()?;
    match cli
        .command
        .unwrap_or_else(|| Command::Play(play::Args::default()))
    {
        Command::Play(args) => play::main(args),
        Command::Hotseat(args) => hotseat::main(args),
    }
}

/// Log to a file: stdout belongs to the UI.
fn init_logging() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let dir = paths::state_dir().context("cannot locate a state directory; is $HOME set?")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never(&dir, "tabletty.log"));
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Ok(guard)
}
