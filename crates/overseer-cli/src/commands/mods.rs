//! `overseer mod ...` subcommands: list, enable, disable, move.

use anyhow::{Context, Result};

use crate::cli::{ModCommand, ProfileArgs};
use crate::context::{load_reconciled, open_instance};
use crate::ui::{heading, list_item, success};

pub fn run(command: ModCommand) -> Result<()> {
    match command {
        ModCommand::List { target } => list(target),
        ModCommand::Enable { name, target } => set_status(target, &name, true),
        ModCommand::Disable { name, target } => set_status(target, &name, false),
        ModCommand::Move { name, to, target } => move_mod(target, &name, to),
    }
}

fn list(target: ProfileArgs) -> Result<()> {
    let instance = open_instance(&target.instance);
    let profile = load_reconciled(&instance, &target.profile)?;

    if profile.mods.is_empty() {
        println!("No mods installed.");
        return Ok(());
    }

    heading(format!(
        "{} - {} mods (highest priority first)",
        profile.name,
        profile.mods.len()
    ));
    for (i, entry) in profile.mods.iter().enumerate() {
        list_item(i + 1, entry.enabled, &entry.name, "");
    }
    Ok(())
}

fn set_status(target: ProfileArgs, mod_name: &str, enabled: bool) -> Result<()> {
    let instance = open_instance(&target.instance);
    let mut profile = load_reconciled(&instance, &target.profile)?;

    if enabled {
        profile.enable(mod_name)
    } else {
        profile.disable(mod_name)
    }
    .with_context(|| {
        format!(
            "{} `{mod_name}`",
            if enabled { "enabling" } else { "disabling" }
        )
    })?;

    profile.save(&instance).context("saving profile")?;
    success(format!(
        "{} `{mod_name}` in profile `{}`",
        if enabled { "Enabled" } else { "Disabled" },
        profile.name
    ));
    Ok(())
}

fn move_mod(target: ProfileArgs, mod_name: &str, to_1based: usize) -> Result<()> {
    let instance = open_instance(&target.instance);
    let mut profile = load_reconciled(&instance, &target.profile)?;

    // The list is presented 1-based; convert to a 0-based index (move_to clamps the rest).
    profile
        .move_to(mod_name, to_1based.saturating_sub(1))
        .with_context(|| format!("moving `{mod_name}`"))?;
    profile.save(&instance).context("saving profile")?;
    success(format!(
        "Moved `{mod_name}` to position {to_1based} in profile `{}`",
        profile.name
    ));
    Ok(())
}
