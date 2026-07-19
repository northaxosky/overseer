//! A profile's managed plugin load order: names and active flags

use super::error::PluginError;
use super::graph::DependencyGraph;
use super::metadata::PluginMeta;
use crate::fs;
use crate::instance::Instance;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// One line of a profile's plugin load order: plugin name and whether it's active
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
}

/// A profile's plugin load order, persisted as `plugins.txt`
#[derive(Debug, Clone, Default)]
pub struct PluginLoadOrder {
    pub profile: String,
    pub plugins: Vec<PluginEntry>,
}

impl PluginLoadOrder {
    /// Load a profile's `plugins.txt`
    pub fn load(instance: &Instance, profile: &str) -> Result<Self, PluginError> {
        let path = instance.profile_dir(profile).join("plugins.txt");
        let text = fs::read_to_string_opt(&path)?.unwrap_or_default();

        Ok(Self {
            profile: profile.to_owned(),
            plugins: parse_plugins(&text),
        })
    }

    /// Write the profile's `plugins.txt`, creating the profile dir if necessary
    pub fn save(&self, instance: &Instance) -> Result<(), PluginError> {
        let path = instance.profile_dir(&self.profile).join("plugins.txt");
        fs::write_atomic(&path, self.to_plugins_string().as_bytes())?;
        Ok(())
    }

    /// Serialize to `plugins.txt` text: `*name` for active, `name` for inactive
    fn to_plugins_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.plugins {
            if entry.active {
                out.push('*');
            }
            out.push_str(&entry.name);
            out.push('\n');
        }
        out
    }

    /// Whether the plugin order already satisfies dependency ordering (reconcile's topological sort)
    pub fn is_dependency_ordered(&self, discovered: &[PluginMeta]) -> bool {
        topological_order(self.plugins.clone(), discovered) == self.plugins
    }

    pub fn position(&self, name: &str) -> Option<usize> {
        self.plugins
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.position(name).is_some_and(|i| self.plugins[i].active)
    }

    fn set_active(&mut self, name: &str, active: bool) -> Result<(), PluginError> {
        let idx = self
            .position(name)
            .ok_or_else(|| PluginError::NotInLoadOrder(name.to_owned()))?;
        self.plugins[idx].active = active;
        Ok(())
    }

    /// Mark a plugin active in the load order
    pub fn activate(&mut self, name: &str) -> Result<(), PluginError> {
        self.set_active(name, true)
    }
    /// Mark a plugin inactive in the load order
    pub fn deactivate(&mut self, name: &str) -> Result<(), PluginError> {
        self.set_active(name, false)
    }

    /// Reconcile the load order with the plugins actually discovered in the profile's enabled mods
    pub fn reconcile(&mut self, discovered: &[PluginMeta]) -> bool {
        // Drop entries that are no longer discovered
        let before_len = self.plugins.len();
        self.plugins.retain(|e| {
            discovered
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(&e.name))
        });
        let mut changed = self.plugins.len() != before_len;

        // Append newly discovered plugins
        for m in discovered {
            if !self.contains(&m.name) {
                self.plugins.push(PluginEntry {
                    name: m.name.clone(),
                    active: true,
                });
                changed = true;
            }
        }

        let pre_dedup = self.plugins.len();
        let mut seen = Vec::<String>::new();
        self.plugins.retain(|entry| {
            if seen
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&entry.name))
            {
                false
            } else {
                seen.push(entry.name.clone());
                true
            }
        });
        changed |= self.plugins.len() != pre_dedup;

        let previous = std::mem::take(&mut self.plugins);
        let previous_snapshot = previous.clone();
        self.plugins = topological_order(previous, discovered);
        changed |= self.plugins != previous_snapshot;
        changed
    }
}

/// Stable topological sort using SCC-external readiness, then master flag and original order.
fn topological_order(entries: Vec<PluginEntry>, discovered: &[PluginMeta]) -> Vec<PluginEntry> {
    let graph = DependencyGraph::build(&entries, discovered);
    let mut external_indegrees = vec![0; entries.len()];
    for (dependency, successors) in graph.successors.iter().enumerate() {
        for &dependant in successors {
            if !graph.same_scc(dependency, dependant) {
                external_indegrees[dependant] += 1;
            }
        }
    }
    let mut ready = BinaryHeap::new();
    let mut emitted = vec![false; entries.len()];
    let mut order = Vec::with_capacity(entries.len());

    for (index, &external_indegree) in external_indegrees.iter().enumerate() {
        if external_indegree == 0 {
            ready.push(Reverse((u8::from(!graph.master_flags[index]), index)));
        }
    }

    while order.len() < entries.len() {
        let Reverse((_, next)) = ready.pop().expect("SCC condensation leaves a ready node");
        emitted[next] = true;
        order.push(next);
        for &successor in &graph.successors[next] {
            if graph.same_scc(next, successor) || emitted[successor] {
                continue;
            }
            external_indegrees[successor] -= 1;
            if external_indegrees[successor] == 0 {
                ready.push(Reverse((
                    u8::from(!graph.master_flags[successor]),
                    successor,
                )));
            }
        }
    }

    let mut entries: Vec<_> = entries.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|index| entries[index].take().expect("entry emitted once"))
        .collect()
}

/// Parse `plugins.txt`
pub(super) fn parse_plugins(text: &str) -> Vec<PluginEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (active, name) = match line.strip_prefix('*') {
                Some(rest) => (true, rest.trim()),
                None => (false, line),
            };
            (!name.is_empty()).then(|| PluginEntry {
                name: name.to_owned(),
                active,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/load_order.rs"]
mod tests;
