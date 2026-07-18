//! `overseer mod ...` profile-order and installed-mod subcommands.

use anyhow::{Context, Result};
use overseer_core::{
    apply,
    lifecycle::{self, LifecycleReport},
};

use crate::cli::{InstanceArgs, ModCommand, ProfileArgs};
use crate::ui::{heading, list_item, success};

pub fn run(command: ModCommand) -> Result<()> {
    match command {
        ModCommand::List { target } => list(&target),
        ModCommand::Enable { name, target } => set_enabled(&target, &name, true),
        ModCommand::Disable { name, target } => set_enabled(&target, &name, false),
        ModCommand::Move { name, to, target } => move_to(&target, &name, to),
        ModCommand::Rename {
            name,
            new_name,
            instance,
        } => rename(&instance, &name, &new_name),
        ModCommand::Remove { name, instance } => remove(&instance, &name),
        ModCommand::Replace {
            name,
            archive,
            instance,
        } => replace(&instance, &name, &archive),
    }
}

fn list(target: &ProfileArgs) -> Result<()> {
    let (_instance, profile) = target.load_context()?;

    let item_count = profile.items().count();
    if item_count == 0 {
        println!("No mods installed");
        return Ok(());
    }

    heading(format!(
        "{} - {} mods (highest priority first)",
        profile.name, item_count
    ));
    for (i, entry) in profile.items().enumerate() {
        list_item(i + 1, entry.enabled, &entry.name, "");
    }
    Ok(())
}

fn set_enabled(target: &ProfileArgs, mod_name: &str, enabled: bool) -> Result<()> {
    let (instance, mut profile) = target.load_context()?;

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

fn move_to(target: &ProfileArgs, mod_name: &str, to_1based: usize) -> Result<()> {
    let (instance, mut profile) = target.load_context()?;

    let ordinal = to_1based.saturating_sub(1);
    let target_row = profile.row_for_item_ordinal(ordinal);
    profile
        .move_to(mod_name, target_row)
        .with_context(|| format!("moving `{mod_name}`"))?;
    profile.save(&instance).context("saving profile")?;
    success(format!(
        "Moved `{mod_name}` to position {to_1based} in profile `{}`",
        profile.name
    ));
    Ok(())
}

fn rename(instance: &InstanceArgs, old: &str, new: &str) -> Result<()> {
    let instance = instance.load_instance()?;
    apply::rename_mod(&instance, old, new)
        .with_context(|| format!("renaming `{old}` to `{new}`"))?;
    success(format!("Renamed mod `{old}` to `{new}`"));
    Ok(())
}

fn remove(instance: &InstanceArgs, name: &str) -> Result<()> {
    let instance = instance.load_instance()?;
    let report =
        lifecycle::remove(&instance, name).with_context(|| format!("removing `{name}`"))?;
    finish_lifecycle("Removed", report);
    Ok(())
}

fn replace(instance: &InstanceArgs, name: &str, archive: &str) -> Result<()> {
    let instance = instance.load_instance()?;
    let report = lifecycle::replace(&instance, name, archive)
        .with_context(|| format!("replacing `{name}` from `{archive}`"))?;
    success(format!("Replaced `{}` from `{archive}`", report.name));
    super::warn_lifecycle_residue(report.residue_warning);
    Ok(())
}

fn finish_lifecycle(verb: &str, report: LifecycleReport) {
    success(format!("{verb} `{}`", report.name));
    super::warn_lifecycle_residue(report.residue_warning);
}
