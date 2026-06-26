//! Plugin metadata and per profile load order

mod discover;
mod error;
mod gamestate;
mod loadorder;
mod metadata;

pub use discover::discover_plugins;
pub use error::PluginError;
pub use gamestate::{
    PluginsRestore, implicit_active_plugins, read_plugins_txt, restore_plugins_txt_if_ours,
    write_active_plugins,
};
pub use loadorder::{PluginEntry, PluginLoadOrder};
pub use metadata::{PluginMeta, is_plugin_file, read_metadata};
