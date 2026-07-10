//! Reading a plugin's header metadata via `esplugin`

use super::error::PluginError;
use camino::Utf8Path;
use esplugin::{GameId, ParseOptions, Plugin};

/// Whether a filename is a Bethesda plugin we manage
pub fn is_plugin_file(name: &str) -> bool {
    matches!(
        Utf8Path::new(name)
            .extension()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("esp" | "esm" | "esl")
    )
}

/// Metadata read from a plugin's header
#[derive(Debug, Clone, PartialEq)]
pub struct PluginMeta {
    /// The plugin's file name
    pub name: String,
    /// A master file: loaded before normal plugins
    pub is_master: bool,
    /// A light (ESL) plugin: shares the `FE` load order slot
    pub is_light: bool,
    /// The plugins this one depends on (masters), in header order
    pub masters: Vec<String>,
    /// The TES4/HEDR module version, if the header carried one
    pub header_version: Option<f32>,
}

/// Whether `name` belongs to a discovered master plugin
pub fn is_master(name: &str, discovered: &[PluginMeta]) -> bool {
    discovered
        .iter()
        .find(|meta| meta.name.eq_ignore_ascii_case(name))
        .is_some_and(|meta| meta.is_master)
}

/// Read a plugin's metadata from its header
pub fn read_metadata(
    game_id: GameId,
    name: &str,
    path: &Utf8Path,
) -> Result<PluginMeta, PluginError> {
    let mut plugin = Plugin::new(game_id, path.as_std_path());
    plugin
        .parse_file(ParseOptions::header_only())
        .map_err(|source| PluginError::Parse {
            path: path.to_owned(),
            source,
        })?;

    let masters = plugin.masters().map_err(|source| PluginError::Parse {
        path: path.to_owned(),
        source,
    })?;

    Ok(PluginMeta {
        name: name.to_owned(),
        is_master: plugin.is_master_file(),
        is_light: plugin.is_light_plugin(),
        masters,
        header_version: plugin.header_version(),
    })
}

#[cfg(test)]
#[path = "tests/metadata.rs"]
mod tests;
