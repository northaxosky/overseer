//! Plugin metadata and per-profile load order

mod discover;
mod error;
mod gamestate;
mod load_order;
mod metadata;

pub use discover::discover_plugins;
pub use error::PluginError;
pub use gamestate::{
    implicit_active_plugins, read_plugins_txt, restore_plugins_txt_if_ours, write_active_plugins,
};
pub use load_order::{PluginEntry, PluginLoadOrder};
pub use metadata::{PluginMeta, is_plugin_file, read_metadata};
