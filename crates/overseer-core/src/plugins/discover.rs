//! Discovering the plugins a profile's enabled mods provide

use super::error::{PluginError, io_err};
use super::metadata::{PluginMeta, is_plugin_file, read_metadata};
use crate::error::non_utf8;
use crate::instance::{Instance, Profile};
use camino::{Utf8Path, Utf8PathBuf};
use walkdir::WalkDir;

/// A plugin file discovery could not parse; carries why for diagnostics
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadablePlugin {
    /// Plugin's filename
    pub name: String,
    /// Why the plugin could not be read or parsed
    pub reason: String,
}

/// Discover the plugins a profile would deploy
pub fn discover_plugins(
    instance: &Instance,
    profile: &Profile,
) -> Result<Vec<PluginMeta>, PluginError> {
    let game_id = instance.config.game.plugin_id();
    discover_plugin_paths(instance, profile)?
        .into_iter()
        .map(|(name, path)| read_metadata(game_id, &name, &path))
        .collect()
}

/// [`discover_plugins`] but keeps going past unparseable plugins, collecting them separately
pub fn discover_plugins_lenient(
    instance: &Instance,
    profile: &Profile,
) -> Result<(Vec<PluginMeta>, Vec<UnreadablePlugin>), PluginError> {
    let game_id = instance.config.game.plugin_id();
    let mut readable = Vec::new();
    let mut unreadable = Vec::new();
    for (name, path) in discover_plugin_paths(instance, profile)? {
        match read_metadata(game_id, &name, &path) {
            Ok(meta) => readable.push(meta),
            Err(error) => unreadable.push(UnreadablePlugin {
                name,
                reason: error.to_string(),
            }),
        }
    }
    Ok((readable, unreadable))
}

/// The deduped `(name, path)` plugin files an enabled mod profile provides, in priority order
fn discover_plugin_paths(
    instance: &Instance,
    profile: &Profile,
) -> Result<Vec<(String, Utf8PathBuf)>, PluginError> {
    let mut seen: Vec<String> = Vec::new();
    let mut paths = Vec::new();
    for entry in profile.items() {
        if !entry.enabled {
            continue;
        }
        let mod_dir = instance.mods_dir().join(&entry.name);
        for found in find_plugin_files(&mod_dir)? {
            let name = found
                .file_name()
                .expect("walked plugin file always has a name")
                .to_owned();
            if seen.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                continue;
            }
            seen.push(name.clone());
            paths.push((name, found));
        }
    }
    Ok(paths)
}

/// Plugin files (`.esp`/`.esm`/`.esl`) directly under a directory; a missing directory yields an empty list
fn find_plugin_files(dir: &Utf8Path) -> Result<Vec<Utf8PathBuf>, PluginError> {
    let mut found = Vec::new();
    for entry in WalkDir::new(dir).min_depth(1).max_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            // A directory that doesn't exist yet (no plugins)
            Err(e)
                if e.io_error().map(std::io::Error::kind) == Some(std::io::ErrorKind::NotFound) =>
            {
                return Ok(found);
            }
            Err(e) => {
                return Err(io_err(dir, e.into()).into());
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = Utf8Path::from_path(entry.path())
            .ok_or_else(|| PluginError::NonUtf8Path(non_utf8(entry.path())))?;
        if let Some(name) = path.file_name()
            && is_plugin_file(name)
        {
            found.push(path.to_owned());
        }
    }
    // WalkDir yields filesystem order; sort so a mod's plugins deploy deterministically
    found.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
    });
    Ok(found)
}

#[cfg(test)]
#[path = "tests/discover.rs"]
mod tests;
