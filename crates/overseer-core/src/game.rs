//! The games Overseer manages and their per engine specifics

use loadorder::GameId;
use serde::{Deserialize, Serialize};

/// A game Overseer can manage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GameKind {
    #[default]
    Fallout4,
    SkyrimSE,
    Starfield,
}

impl GameKind {
    /// The LOOT stack's id, for load order rules (`libloadorder`)
    pub fn load_order_id(self) -> GameId {
        match self {
            GameKind::Fallout4 => GameId::Fallout4,
            GameKind::SkyrimSE => GameId::SkyrimSE,
            GameKind::Starfield => GameId::Starfield,
        }
    }

    /// The plugin perser id (`esplugin`), from the load order id
    pub fn plugin_id(self) -> esplugin::GameId {
        self.load_order_id().to_esplugin_id()
    }

    /// Folder under `%LOCALAPPDATA%` where the game keeps `Plugins.txt`
    pub fn local_appdata_dir(self) -> &'static str {
        match self {
            GameKind::Fallout4 => "Fallout4",
            GameKind::SkyrimSE => "Skyrim Special Edition",
            GameKind::Starfield => "Starfield",
        }
    }
}

impl std::fmt::Display for GameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            GameKind::Fallout4 => "Fallout 4",
            GameKind::SkyrimSE => "Skyrim Special Edition",
            GameKind::Starfield => "Starfield",
        };
        f.write_str(name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_fallout4() {
        assert_eq!(GameKind::default(), GameKind::Fallout4);
    }

    #[test]
    fn maps_to_load_order_ids() {
        assert!(matches!(
            GameKind::Fallout4.load_order_id(),
            GameId::Fallout4
        ));
        assert!(matches!(
            GameKind::SkyrimSE.load_order_id(),
            GameId::SkyrimSE
        ));
        assert!(matches!(
            GameKind::Starfield.load_order_id(),
            GameId::Starfield
        ));
    }

    #[test]
    fn derives_esplugin_ids_from_the_load_order_id() {
        assert!(matches!(
            GameKind::Fallout4.plugin_id(),
            esplugin::GameId::Fallout4
        ));
        assert!(matches!(
            GameKind::SkyrimSE.plugin_id(),
            esplugin::GameId::SkyrimSE
        ));
        assert!(matches!(
            GameKind::Starfield.plugin_id(),
            esplugin::GameId::Starfield
        ));
    }

    #[test]
    fn local_appdata_dirs_match_the_games() {
        assert_eq!(GameKind::Fallout4.local_appdata_dir(), "Fallout4");
        assert_eq!(
            GameKind::SkyrimSE.local_appdata_dir(),
            "Skyrim Special Edition"
        );
        assert_eq!(GameKind::Starfield.local_appdata_dir(), "Starfield");
    }

    #[test]
    fn serializes_by_bare_variant_name() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Wrap {
            game: GameKind,
        }

        // Instances persist this in overseer.toml, so the on-disk form must stay
        // stable -- a rename here would silently break existing instances.
        let toml_str = toml::to_string(&Wrap {
            game: GameKind::Starfield,
        })
        .expect("serialize");
        assert_eq!(toml_str, "game = \"Starfield\"\n");

        let back: Wrap = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(back.game, GameKind::Starfield);
    }
}
