#![cfg_attr(test, allow(clippy::unwrap_used))]

use anyhow::Result;
use clap::Parser;

mod cli;
mod cli_upgrade;
mod commands;

#[cfg(test)]
mod cli_tests;

use cli::Cli;
use commands::*;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    reject_removed_output_flags();

    let mut cli = Cli::parse();
    cli.json = true;
    commands::run(&cli)
}
