//! Overseer CLI: argument parsing and dispatch. Command logic lives in `commands/`;

mod cli;
mod commands;
mod context;
mod ui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    overseer_frontend::logging::init(overseer_frontend::logging::Config {
        default_filter: "warn,overseer=info,overseer_core=info",
        warn_on_error: true,
    });
    tracing::info!("overseer-cli starting");
    let cli = Cli::parse();
    ui::apply_color_choice(cli.color);
    match cli.command {
        Command::Demo => commands::demo::run(),
        Command::Deploy { target } => commands::deploy::deploy(&target),
        Command::Purge { instance } => commands::deploy::purge(instance),
        Command::Install {
            archive,
            instance,
            name,
        } => commands::install::run(archive, instance, name),
        Command::Mod { command } => commands::mods::run(command),
        Command::Plugin { command } => commands::plugins::run(command),
        Command::Profile { command } => commands::profile::run(command),
        Command::Instance { command } => commands::instance::run(command),
        Command::Status { instance } => commands::deploy::status(instance),
        Command::Launch { name, instance } => commands::launch::run(name, instance),
        Command::Exe { command } => commands::exe::run(command),
        Command::Doctor { target } => commands::doctor::run(&target),
    }
}
