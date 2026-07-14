//! The real game load order: writing the game's `Plugins.txt` via libloadorder

use super::error::PluginError;
use super::load_order::PluginEntry;
use crate::restore::{Restore, restore_if_ours};
use camino::{Utf8Path, Utf8PathBuf};
use loadorder::{GameId, GameSettings};

/// Write the game's real `Plugins.txt` to match `plugins` (load order + active flags)
pub fn write_active_plugins(
    game_id: GameId,
    game_dir: &Utf8Path,
    local_dir: &Utf8Path,
    plugins: &[PluginEntry],
) -> Result<(), PluginError> {
    let settings =
        GameSettings::with_local_path(game_id, game_dir.as_std_path(), local_dir.as_std_path())?;
    let mut load_order = settings.into_load_order();
    load_order.load()?;

    let order: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    load_order.set_load_order(&order)?;

    let active: Vec<&str> = plugins
        .iter()
        .filter(|p| p.active)
        .map(|p| p.name.as_str())
        .collect();
    load_order.set_active_plugins(&active)?;
    load_order.save()?;
    Ok(())
}

/// The plugins this game force-loads regardless of `Plugins.txt`
pub fn implicit_active_plugins(
    game_id: GameId,
    game_dir: &Utf8Path,
    local_dir: &Utf8Path,
) -> Result<Vec<String>, PluginError> {
    let settings =
        GameSettings::with_local_path(game_id, game_dir.as_std_path(), local_dir.as_std_path())?;
    Ok(settings.implicitly_active_plugins().to_vec())
}

/// The game's real `Plugins.txt` lives directly in the local data dir
fn plugins_txt_path(local_dir: &Utf8Path) -> Utf8PathBuf {
    local_dir.join("Plugins.txt")
}

/// Read the current real `Plugins.txt` so it can be restored later
pub fn read_plugins_txt(local_dir: &Utf8Path) -> Result<Option<Vec<u8>>, PluginError> {
    Ok(crate::fs::read_opt(&plugins_txt_path(local_dir))?)
}

/// Restore `Plugins.txt`: `Some(bytes)` rewrites original; `None` removes the file we created
pub(crate) fn restore_plugins_txt(
    local_dir: &Utf8Path,
    backup: Option<&[u8]>,
) -> Result<(), PluginError> {
    let path = plugins_txt_path(local_dir);
    match backup {
        Some(bytes) => crate::fs::write(&path, bytes)?,
        None => crate::fs::remove_file_opt(&path)?,
    }
    Ok(())
}

/// Restore the user's original `Plugins.txt`, only when the live file is still the one this deployment wrote
pub fn restore_plugins_txt_if_ours(
    local_dir: &Utf8Path,
    original: Option<&[u8]>,
    intended: Option<&[u8]>,
) -> Result<Restore, PluginError> {
    restore_if_ours(
        intended.map(|bytes| Some(bytes.to_vec())),
        original.map(<[u8]>::to_vec),
        || Ok((read_plugins_txt(local_dir)?, Some(()))),
        |_| restore_plugins_txt(local_dir, original),
    )
}

#[cfg(test)]
#[path = "tests/gamestate.rs"]
mod tests;
