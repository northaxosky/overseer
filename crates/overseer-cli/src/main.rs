//! Overseer CLI: argument parsing and dispatch. Command logic lives in `commands/`;

mod cli;
mod commands;
mod context;
mod ui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Demo => commands::demo::run(),
        Command::Deploy {
            target,
            mods,
            manifest,
        } => commands::deploy::deploy(target, mods, manifest),
        Command::Purge { manifest } => commands::deploy::purge(manifest),
        Command::Install {
            archive,
            instance,
            name,
        } => commands::install::run(archive, instance, name),
        Command::Mod { command } => commands::mods::run(command),
        Command::Plugin { command } => commands::plugins::run(command),
        Command::Instance { command } => commands::instance::run(command),
    }
}
