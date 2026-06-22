//! The games Overseer manages and their per engine specifics

use loadorder::GameId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
            Self::Fallout4 => GameId::Fallout4,
            Self::SkyrimSE => GameId::SkyrimSE,
            Self::Starfield => GameId::Starfield,
        }
    }

    /// The plugin perser id (`esplugin`), from the load order id
    pub fn plugin_id(self) -> esplugin::GameId {
        self.load_order_id().to_esplugin_id()
    }

    /// The game's main executable, found in the install root
    pub fn executable(self) -> &'static str {
        match self {
            Self::Fallout4 => "Fallout4.exe",
            Self::SkyrimSE => "SkyrimSE.exe",
            Self::Starfield => "Starfield.exe",
        }
    }

    /// The script extender loader (F4SE, SKSE64, SFSE)
    pub fn script_extender_loader(self) -> &'static str {
        match self {
            Self::Fallout4 => "f4se_loader.exe",
            Self::SkyrimSE => "skse64_loader.exe",
            Self::Starfield => "sfse_loader.exe",
        }
    }

    /// Folder under `%LOCALAPPDATA%` where the game keeps `Plugins.txt`
    pub fn local_appdata_dir(self) -> &'static str {
        match self {
            Self::Fallout4 => "Fallout4",
            Self::SkyrimSE => "Skyrim Special Edition",
            Self::Starfield => "Starfield",
        }
    }

    /// The Creation Club load-order manifest in the game root, if the game uses one
    pub fn ccc_file(self) -> Option<&'static str> {
        match self {
            Self::Fallout4 => Some("Fallout4.ccc"),
            Self::SkyrimSE => Some("Skyrim.ccc"),
            Self::Starfield => None,
        }
    }
}

impl std::fmt::Display for GameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Fallout4 => "Fallout 4",
            Self::SkyrimSE => "Skyrim Special Edition",
            Self::Starfield => "Starfield",
        };
        f.write_str(name)
    }
}

/// Returned when a string does not name a game Overseer supports
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown game '{0}' (expected one of: fallout4, skyrimse, starfield)")]
pub struct ParseGameKindError(String);

impl FromStr for GameKind {
    type Err = ParseGameKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "fallout4" | "fo4" => Ok(Self::Fallout4),
            "skyrimse" | "sse" => Ok(Self::SkyrimSE),
            "starfield" | "sf" => Ok(Self::Starfield),
            _ => Err(ParseGameKindError(s.to_owned())),
        }
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
    fn executables_and_loaders_per_game() {
        assert_eq!(GameKind::Fallout4.executable(), "Fallout4.exe");
        assert_eq!(GameKind::SkyrimSE.executable(), "SkyrimSE.exe");
        assert_eq!(GameKind::Starfield.executable(), "Starfield.exe");

        assert_eq!(
            GameKind::Fallout4.script_extender_loader(),
            "f4se_loader.exe"
        );
        assert_eq!(
            GameKind::SkyrimSE.script_extender_loader(),
            "skse64_loader.exe"
        );
        assert_eq!(
            GameKind::Starfield.script_extender_loader(),
            "sfse_loader.exe"
        );
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
    fn ccc_files_match_the_games() {
        assert_eq!(GameKind::Fallout4.ccc_file(), Some("Fallout4.ccc"));
        assert_eq!(GameKind::SkyrimSE.ccc_file(), Some("Skyrim.ccc"));
        assert_eq!(GameKind::Starfield.ccc_file(), None);
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

    #[test]
    fn parses_canonical_names_case_insensitively() {
        assert_eq!("fallout4".parse::<GameKind>().unwrap(), GameKind::Fallout4);
        assert_eq!("SkyrimSE".parse::<GameKind>().unwrap(), GameKind::SkyrimSE);
        assert_eq!(
            "STARFIELD".parse::<GameKind>().unwrap(),
            GameKind::Starfield
        );
    }

    #[test]
    fn parses_short_aliases() {
        assert_eq!("fo4".parse::<GameKind>().unwrap(), GameKind::Fallout4);
        assert_eq!("sse".parse::<GameKind>().unwrap(), GameKind::SkyrimSE);
        assert_eq!("sf".parse::<GameKind>().unwrap(), GameKind::Starfield);
    }

    #[test]
    fn rejects_unknown_game_with_a_helpful_message() {
        let err = "morrowind".parse::<GameKind>().unwrap_err();
        assert_eq!(err, ParseGameKindError("morrowind".to_owned()));
        assert!(err.to_string().contains("morrowind"));
    }
}
