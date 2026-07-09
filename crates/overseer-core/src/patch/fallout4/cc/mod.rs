//! Fallout 4 Creation Club policy: the bundled allow-list and which load order plugins are CC

mod catalog;
pub use catalog::is_cc;

use crate::plugins::PluginLoadOrder;

/// The Creation Club plugins in a load order, order preserved, active and inactive alike
pub fn cc_plugins(order: &PluginLoadOrder) -> Vec<String> {
    order
        .plugins
        .iter()
        .filter(|entry| catalog::is_cc(&entry.name))
        .map(|entry| entry.name.clone())
        .collect()
}

#[cfg(test)]
#[path = "tests/cc.rs"]
mod tests;
