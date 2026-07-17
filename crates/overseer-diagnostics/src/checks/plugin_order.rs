//! Plugin load-order violations found by the core validator

use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::plugins::{PluginViolation, validate_order};

/// Reports order-dependent plugin problems
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    validate_order(&ctx.plugin_order, &ctx.discovered_plugins)
        .into_iter()
        .map(finding)
        .collect()
}

fn finding(violation: PluginViolation) -> Finding {
    let severity = violation.severity().into();
    let title = match violation {
        PluginViolation::DependencyAfterDependant { plugin, dependency } => {
            format!("`{dependency}` loads after dependant `{plugin}`")
        }
        PluginViolation::MasterAfterNormal(plugin) => {
            format!("Master plugin `{plugin}` loads after a normal plugin")
        }
        PluginViolation::DuplicatePlugin(name) => {
            format!("Plugin `{name}` appears more than once in the load order")
        }
        PluginViolation::OrderReferencesMissing(name) => {
            format!("Load order references missing plugin `{name}`")
        }
        PluginViolation::CyclicDependency(members) => {
            format!("Dependency cycle: {}", members.join(", "))
        }
    };
    Finding::new(severity, title)
}

#[cfg(test)]
#[path = "tests/plugin_order.rs"]
mod tests;
