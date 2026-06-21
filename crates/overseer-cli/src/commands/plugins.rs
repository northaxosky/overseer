//! `overseer plugin ...` subcommands: list, activate, deactivate.

use anyhow::{Context, Result};
use overseer_core::instance::Instance;
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};

use crate::cli::{PluginCommand, ProfileArgs};
use crate::context::{load_reconciled, open_instance};
use crate::ui::{heading, list_item, success};

pub fn run(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::List { target } => list(&target),
        PluginCommand::Activate { name, target } => set_active(&target, &name, true),
        PluginCommand::Deactivate { name, target } => set_active(&target, &name, false),
    }
}

/// Reconcile the mod list, discover plugins from enabled mods, and load + reconcile the plugin load order.
fn synced(instance: &Instance, profile_name: &str) -> Result<(Vec<PluginMeta>, PluginLoadOrder)> {
    let profile = load_reconciled(instance, profile_name)?;
    let discovered = discover_plugins(instance, &profile).context("discovering plugins")?;
    let mut order = PluginLoadOrder::load(instance, profile_name)
        .with_context(|| format!("loading plugins.txt for `{profile_name}`"))?;
    if order.reconcile(&discovered) {
        order.save(instance).context("saving plugins.txt")?;
    }
    Ok((discovered, order))
}

fn list(target: &ProfileArgs) -> Result<()> {
    let instance = open_instance(&target.instance)?;
    let (discovered, order) = synced(&instance, &target.profile)?;

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
    let instance = open_instance(&target.instance)?;
    let (_discovered, mut order) = synced(&instance, &target.profile)?;

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
