//! Pure validation for plugin load-order constraints

use super::graph::DependencyGraph;
use super::{PluginLoadOrder, PluginMeta};
use std::collections::{HashMap, HashSet};

/// Severity of a plugin order violation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A problem with a plugin load order
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginViolation {
    /// A declared dependency loads after the plugin that needs it
    DependencyAfterDependant { plugin: String, dependency: String },
    /// A master plugin is ordered after a non-master
    MasterAfterNormal(String),
    /// The same plugin appears more than once
    DuplicatePlugin(String),
    /// An order entry names a plugin that is not discovered
    OrderReferencesMissing(String),
    /// Plugins contain a declared dependency cycle
    CyclicDependency(Vec<String>),
}

impl PluginViolation {
    /// The severity this violation is reported at.
    pub fn severity(&self) -> Severity {
        match self {
            Self::OrderReferencesMissing(_) => Severity::Warning,
            Self::DependencyAfterDependant { .. }
            | Self::MasterAfterNormal(_)
            | Self::DuplicatePlugin(_)
            | Self::CyclicDependency(_) => Severity::Error,
        }
    }
}

/// Report order-dependent plugin problems without reading or writing state
pub fn validate_order(order: &PluginLoadOrder, discovered: &[PluginMeta]) -> Vec<PluginViolation> {
    let graph = DependencyGraph::build(&order.plugins, discovered);
    let counts: HashMap<String, usize> =
        order
            .plugins
            .iter()
            .fold(HashMap::new(), |mut counts, entry| {
                *counts.entry(entry.name.to_ascii_lowercase()).or_default() += 1;
                counts
            });

    let cycles = graph.cyclic_components();
    let cyclic_plugins: HashSet<_> = cycles
        .iter()
        .flat_map(|component| component.iter())
        .map(|&index| graph.key(index).to_owned())
        .collect();
    let mut violations: Vec<_> = cycles
        .into_iter()
        .map(|component| {
            PluginViolation::CyclicDependency(
                component
                    .into_iter()
                    .map(|index| order.plugins[index].name.clone())
                    .collect(),
            )
        })
        .collect();
    let mut pending_normals = HashSet::new();
    let mut dependency_pairs = HashSet::new();
    let mut reported_masters = HashSet::new();
    let mut reported_duplicates = HashSet::new();
    let mut reported_missing = HashSet::new();

    for (index, entry) in order.plugins.iter().enumerate() {
        let key = entry.name.to_ascii_lowercase();
        let master = graph.master_flags[index];

        if entry.active
            && !cyclic_plugins.contains(&key)
            && let Some(meta) = graph.metadata(index)
        {
            let plugin_pos = graph.node_index(&key).expect("entry has a node");
            for dependency in &meta.masters {
                let dependency_key = dependency.to_ascii_lowercase();
                if graph
                    .node_index(&dependency_key)
                    .is_some_and(|position| position > plugin_pos)
                    && dependency_pairs.insert((key.clone(), dependency_key))
                {
                    violations.push(PluginViolation::DependencyAfterDependant {
                        plugin: entry.name.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        if master {
            if let Some(meta) = graph.metadata(index) {
                for dependency in &meta.masters {
                    pending_normals.remove(&dependency.to_ascii_lowercase());
                }
            }
            if !pending_normals.is_empty() && reported_masters.insert(key.clone()) {
                violations.push(PluginViolation::MasterAfterNormal(entry.name.clone()));
            }
        } else {
            pending_normals.insert(key.clone());
        }
        if counts[&key] > 1 && reported_duplicates.insert(key.clone()) {
            violations.push(PluginViolation::DuplicatePlugin(entry.name.clone()));
        }
        if graph.metadata(index).is_none() && reported_missing.insert(key) {
            violations.push(PluginViolation::OrderReferencesMissing(entry.name.clone()));
        }
    }

    violations
}

#[cfg(test)]
#[path = "tests/validate.rs"]
mod tests;
