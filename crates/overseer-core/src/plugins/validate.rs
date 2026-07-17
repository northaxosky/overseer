//! Pure validation for plugin load-order constraints

use super::{PluginLoadOrder, PluginMeta, is_master};
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
}

impl PluginViolation {
    pub fn severity(&self) -> Severity {
        match self {
            Self::OrderReferencesMissing(_) => Severity::Warning,
            Self::DependencyAfterDependant { .. }
            | Self::MasterAfterNormal(_)
            | Self::DuplicatePlugin(_) => Severity::Error,
        }
    }
}

/// Report order-dependent plugin problems without reading or writing state
pub fn validate_order(order: &PluginLoadOrder, discovered: &[PluginMeta]) -> Vec<PluginViolation> {
    let positions: HashMap<String, usize> =
        order
            .plugins
            .iter()
            .enumerate()
            .fold(HashMap::new(), |mut positions, (index, entry)| {
                positions
                    .entry(entry.name.to_ascii_lowercase())
                    .or_insert(index);
                positions
            });
    let metadata: HashMap<String, &PluginMeta> =
        discovered
            .iter()
            .fold(HashMap::new(), |mut metadata, meta| {
                metadata
                    .entry(meta.name.to_ascii_lowercase())
                    .or_insert(meta);
                metadata
            });
    let counts: HashMap<String, usize> =
        order
            .plugins
            .iter()
            .fold(HashMap::new(), |mut counts, entry| {
                *counts.entry(entry.name.to_ascii_lowercase()).or_default() += 1;
                counts
            });

    let mut violations = Vec::new();
    let mut saw_normal = false;
    let mut dependency_pairs = HashSet::new();
    let mut reported_masters = HashSet::new();
    let mut reported_duplicates = HashSet::new();
    let mut reported_missing = HashSet::new();

    for entry in &order.plugins {
        let key = entry.name.to_ascii_lowercase();
        let master = is_master(&entry.name, discovered);

        if entry.active
            && let Some(meta) = metadata.get(&key)
        {
            let plugin_pos = positions[&key];
            for dependency in &meta.masters {
                let dependency_key = dependency.to_ascii_lowercase();
                if positions
                    .get(&dependency_key)
                    .is_some_and(|position| *position > plugin_pos)
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
            if saw_normal && reported_masters.insert(key.clone()) {
                violations.push(PluginViolation::MasterAfterNormal(entry.name.clone()));
            }
        } else {
            saw_normal = true;
        }
        if counts[&key] > 1 && reported_duplicates.insert(key.clone()) {
            violations.push(PluginViolation::DuplicatePlugin(entry.name.clone()));
        }
        if !metadata.contains_key(&key) && reported_missing.insert(key) {
            violations.push(PluginViolation::OrderReferencesMissing(entry.name.clone()));
        }
    }

    violations
}

#[cfg(test)]
#[path = "tests/validate.rs"]
mod tests;
