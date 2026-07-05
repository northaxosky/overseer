//! Discovering the plugins a profile's enabled mods provide

use super::error::{PluginError, io_err};
use super::metadata::{PluginMeta, is_plugin_file, read_metadata};
use crate::error::non_utf8;
use crate::instance::{Instance, Profile};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Discover the plugins a profile would deploy
pub fn discover_plugins(
    instance: &Instance,
    profile: &Profile,
) -> Result<Vec<PluginMeta>, PluginError> {
    let mut seen: Vec<String> = Vec::new();
    let mut plugins: Vec<PluginMeta> = Vec::new();
    let game_id = instance.config.game.plugin_id();

    for entry in &profile.mods {
        if !entry.enabled {
            continue;
        }
        let mod_dir = instance.mods_dir().join(&entry.name);
        for found in find_plugin_files(&mod_dir)? {
            let name = found
                .file_name()
                .expect("Walked plugin file always has a name")
                .to_owned();

            if seen.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                continue;
            }
            plugins.push(read_metadata(game_id, &name, &found)?);
            seen.push(name);
        }
    }

    Ok(plugins)
}

/// Plugin files (`.esp`/`.esm`/`.esl`) directly under a directory; a missing directory yields an empty list
fn find_plugin_files(dir: &Utf8Path) -> Result<Vec<camino::Utf8PathBuf>, PluginError> {
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
