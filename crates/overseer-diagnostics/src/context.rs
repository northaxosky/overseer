//! Facts about the setup, gathered once and shared by every check

use crate::error::DiagnosticError;
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, PluginMeta, discover_plugins};

/// The state a diagnostic run inspects. Gathered once using [`GameContext::gather`]
pub struct GameContext {
    /// The plugins the game will actually load (active, with metadata)
    pub active_plugins: Vec<PluginMeta>,
}

impl GameContext {
    /// Gather the context for one profile
    pub fn gather(instance: &Instance, profile: &str) -> Result<Self, DiagnosticError> {
        let mut profile = Profile::load(instance, profile)?;
        profile.reconcile(instance)?;

        let discovered = discover_plugins(instance, &profile)?;

        let mut order = PluginLoadOrder::load(instance, &profile.name)?;
        order.reconcile(&discovered);

        let active_plugins = discovered
            .into_iter()
            .filter(|p| order.is_active(&p.name))
            .collect();

        Ok(Self { active_plugins })
    }
}
