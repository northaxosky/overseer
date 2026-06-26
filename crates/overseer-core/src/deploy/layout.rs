//! The Root/Data deployment-layout convention, shared across the engine

use camino::{Utf8Path, Utf8PathBuf};

/// The game's main asset directory, relative to the game root
pub const DATA_DIR: &str = "Data";

/// A mod's root-deploy folder; its contents map to the game root instead of `Data/`
pub const ROOT_DIR: &str = "Root";

/// If `game_relative` starts with a `Data` component, return the part after it
pub fn strip_data_prefix(game_relative: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut components = game_relative.components();
    match components.next() {
        Some(first) if first.as_str().eq_ignore_ascii_case(DATA_DIR) => {
            Some(components.as_path().to_owned())
        }
        _ => None,
    }
}
