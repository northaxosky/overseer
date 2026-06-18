//! Plugin metadata and per profile load order

mod error;
mod metadata;

pub use error::PluginError;
pub use metadata::{PluginMeta, is_plugin_file, read_metadata};
