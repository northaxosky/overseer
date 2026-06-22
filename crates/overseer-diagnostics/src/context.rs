//! Facts about the setup, gathered once and shared by every check

use crate::error::DiagnosticError;
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins, find_plugin_files};
use std::collections::BTreeSet;

/// The state a diagnostic run inspects. Gathered once using [`GameContext::gather`]
pub struct GameContext {
    /// The active mod plugins to inspect (with their masters)
    pub active_plugins: Vec<PluginMeta>,
    /// Lowercased names of every plugin present when the game loads
    pub present_plugins: BTreeSet<String>,
}

impl GameContext {
    /// Gather the context for one profile
    pub fn gather(instance: &Instance, profile: &str) -> Result<Self, DiagnosticError> {
        let mut profile = Profile::load(instance, profile)?;
        profile.reconcile(instance)?;

        let discovered = discover_plugins(instance, &profile)?;
        let mut order = PluginLoadOrder::load(instance, &profile.name)?;
        order.reconcile(&discovered);

        let active_plugins: Vec<PluginMeta> = discovered
            .into_iter()
            .filter(|p| order.is_active(&p.name))
            .collect();

        // Everything a master can resolve against at load time: the real Data/ folder
        // (vanilla + owned DLC + CC) plus the active mod plugins.
        let mut present_plugins: BTreeSet<String> =
            find_plugin_files(&instance.config.game_dir.join("Data"))?
                .iter()
                .filter_map(|p| p.file_name())
                .map(str::to_lowercase)
                .collect();
        present_plugins.extend(active_plugins.iter().map(|p| p.name.to_lowercase()));

        Ok(Self {
            active_plugins,
            present_plugins,
        })
    }
}
