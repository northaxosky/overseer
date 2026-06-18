//! Plugin metadata and per profile load order

mod discover;
mod error;
mod loadorder;
mod metadata;

#[cfg(test)]
mod test_support;

pub use discover::discover_plugins;
pub use error::PluginError;
pub use loadorder::{PluginEntry, PluginLoadOrder};
pub use metadata::{PluginMeta, is_plugin_file, read_metadata};
