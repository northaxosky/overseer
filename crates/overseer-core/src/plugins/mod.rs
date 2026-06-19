//! Plugin metadata and per profile load order

mod discover;
mod error;
mod gamestate;
mod loadorder;
mod metadata;

#[cfg(test)]
pub(crate) mod test_support;

pub use discover::discover_plugins;
pub use error::PluginError;
pub use gamestate::{read_plugins_txt, restore_plugins_txt, write_active_plugins};
pub use loadorder::{PluginEntry, PluginLoadOrder};
pub use metadata::{PluginMeta, is_plugin_file, read_metadata};
