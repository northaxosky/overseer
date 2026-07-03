//! `overseer plugin ...` subcommands: list, activate, deactivate.

use anyhow::{Context, Result};
use overseer_core::instance::Instance;
use overseer_core::plugins::{PluginLoadOrder, PluginMeta};

use crate::cli::{PluginCommand, ProfileArgs};
use crate::ui::{heading, list_item, success};

pub fn run(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::List { target } => list(&target),
        PluginCommand::Activate { name, target } => set_active(&target, &name, true),
        PluginCommand::Deactivate { name, target } => set_active(&target, &name, false),
    }
}

/// Reconcile the mod list, discover plugins from enabled mods, and load + reconcile the plugin load order.
fn synced(target: &ProfileArgs) -> Result<(Instance, Vec<PluginMeta>, PluginLoadOrder)> {
    let (instance, profile) = target.load_context()?;
    let (discovered, order) = profile
        .sync_plugins(&instance)
        .with_context(|| format!("syncing plugins for `{}`", profile.name))?;
    Ok((instance, discovered, order))
}

fn list(target: &ProfileArgs) -> Result<()> {
    let (_instance, discovered, order) = synced(target)?;

    if order.plugins.is_empty() {
        println!("No plugins. (Install mods with plugins and enable them.)");
        return Ok(());
    }

    heading(format!(
        "{} - {} plugins (load order; masters first)",
        target.profile,
        order.plugins.len()
    ));
    for (i, entry) in order.plugins.iter().enumerate() {
        let is_master = discovered
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&entry.name) && m.is_master);
        let tag = if is_master { " (master)" } else { "" };
        list_item(i + 1, entry.active, &entry.name, tag);
    }
    Ok(())
}

fn set_active(target: &ProfileArgs, plugin: &str, active: bool) -> Result<()> {
    let (instance, _discovered, mut order) = synced(target)?;

    if active {
        order.activate(plugin)
    } else {
        order.deactivate(plugin)
    }
    .with_context(|| {
        format!(
            "{} `{plugin}`",
            if active { "activating" } else { "deactivating" }
        )
    })?;

    order.save(&instance).context("saving plugins.txt")?;
    success(format!(
        "{} `{plugin}` in profile `{}`",
        if active { "Activated" } else { "Deactivated" },
        target.profile
    ));
    Ok(())
}
