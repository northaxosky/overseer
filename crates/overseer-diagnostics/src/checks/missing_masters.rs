//! Active plugins whose masters aren't present; the game won't load

use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::plugins::PluginMeta;
use std::collections::BTreeSet;

/// Flags any active plugin that depends on a master which isn't present
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    // A master is satisfied only if its provider is actually loaded: the active mod plugins plus the base/DLC/CC the engine force-loads (not merely on disk)
    let loaded: BTreeSet<String> = ctx
        .loaded_plugins
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect();
    let mut findings: Vec<Finding> = ctx
        .active_plugins
        .iter()
        .filter_map(|plugin| missing_for(plugin, &loaded))
        .collect();

    if findings.is_empty() {
        findings.push(Finding::info("All plugin masters are present"));
    }
    findings
}

/// A finding for one plugin if any of its masters isn't present
fn missing_for(plugin: &PluginMeta, present: &BTreeSet<String>) -> Option<Finding> {
    let missing: Vec<String> = plugin
        .masters
        .iter()
        .filter(|m| !present.contains(&m.to_lowercase()))
        .map(|m| format!("`{m}`"))
        .collect();

    if missing.is_empty() {
        return None;
    }
    Some(
        Finding::error(format!(
            "`{}` is missing {}",
            plugin.name,
            missing.join(", ")
        ))
        .detail("Install or activate the master(s), or deactivate this plugin"),
    )
}

#[cfg(test)]
#[path = "tests/missing_masters.rs"]
mod tests;
