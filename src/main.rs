use clap::Parser;
use schemata::cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Convert { input, output } => {
            tracing::info!(?input, ?output, "starting conversion");
            // Pipeline stages will be wired here in later phases.
            tracing::warn!("conversion pipeline not yet implemented");
            Ok(())
        }
    }
}
