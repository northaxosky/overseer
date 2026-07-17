//! A profile's managed plugin load order: names and active flags

use super::error::PluginError;
use super::metadata::{PluginMeta, is_master};
use crate::fs;
use crate::instance::Instance;

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

        // Stable sort masters before normal plugins, only when not already ordered
        if !self
            .plugins
            .is_sorted_by_key(|e| !is_master(&e.name, discovered))
        {
            self.plugins
                .sort_by_key(|e| !is_master(&e.name, discovered));
            changed = true;
        }
        changed
    }
}

/// Parse `plugins.txt`
fn parse_plugins(text: &str) -> Vec<PluginEntry> {
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
