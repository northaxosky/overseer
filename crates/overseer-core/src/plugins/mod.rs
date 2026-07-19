//! Plugin metadata and per-profile load order

mod carrier;
mod discover;
mod error;
mod gamestate;
mod graph;
mod load_order;
mod metadata;
mod provider;
mod separator;
pub mod validate;

pub use carrier::carrier_for;
pub use discover::{UnreadablePlugin, discover_plugins, discover_plugins_lenient};
pub use error::PluginError;
pub(crate) use gamestate::{decode_plugins_txt, restore_plugins_txt};
pub use gamestate::{
    implicit_active_plugins, read_plugins_txt, restore_plugins_txt_if_ours, write_active_plugins,
};
pub use load_order::{PluginEntry, PluginLoadOrder};
pub use metadata::{PluginMeta, is_master, is_plugin_file, read_metadata};
pub use provider::plugin_provider;
pub use separator::{PluginRow, PluginSeparators, Separator, SeparatorError, merge_rows};
pub use validate::{PluginViolation, Severity, validate_order};
