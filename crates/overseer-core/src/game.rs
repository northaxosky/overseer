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

/// The per-game constants, declared once per variant so adding a game touches one place.
struct GameSpecs {
    load_order_id: GameId,
    executable: &'static str,
    script_extender_loader: &'static str,
    /// Folder name under both `%LOCALAPPDATA%` and `Documents/My Games` (identical per game).
    data_dir_name: &'static str,
    ini_stem: &'static str,
    ccc_file: Option<&'static str>,
    display_name: &'static str,
}

impl GameKind {
    /// All per-game specifics in one place; every accessor below reads a field from here.
    fn specs(self) -> GameSpecs {
        match self {
            Self::Fallout4 => GameSpecs {
                load_order_id: GameId::Fallout4,
                executable: "Fallout4.exe",
                script_extender_loader: "f4se_loader.exe",
                data_dir_name: "Fallout4",
                ini_stem: "Fallout4",
                ccc_file: Some("Fallout4.ccc"),
                display_name: "Fallout 4",
            },
            Self::SkyrimSE => GameSpecs {
                load_order_id: GameId::SkyrimSE,
                executable: "SkyrimSE.exe",
                script_extender_loader: "skse64_loader.exe",
                data_dir_name: "Skyrim Special Edition",
                ini_stem: "Skyrim",
                ccc_file: Some("Skyrim.ccc"),
                display_name: "Skyrim Special Edition",
            },
            Self::Starfield => GameSpecs {
                load_order_id: GameId::Starfield,
                executable: "Starfield.exe",
                script_extender_loader: "sfse_loader.exe",
                data_dir_name: "Starfield",
                ini_stem: "Starfield",
                ccc_file: None,
                display_name: "Starfield",
            },
        }
    }

    /// The LOOT stack's id, for load order rules (`libloadorder`)
    pub fn load_order_id(self) -> GameId {
        self.specs().load_order_id
    }

    /// The plugin perser id (`esplugin`), from the load order id
    pub fn plugin_id(self) -> esplugin::GameId {
        self.load_order_id().to_esplugin_id()
    }

    /// The game's main executable, found in the install root
    pub fn executable(self) -> &'static str {
        self.specs().executable
    }

    /// The script extender loader (F4SE, SKSE64, SFSE)
    pub fn script_extender_loader(self) -> &'static str {
        self.specs().script_extender_loader
    }

    /// Folder under `%LOCALAPPDATA%` where the game keeps `Plugins.txt`
    pub fn local_appdata_dir(self) -> &'static str {
        self.specs().data_dir_name
    }

    /// The Creation Club load-order manifest in the game root, if the game uses one
    pub fn ccc_file(self) -> Option<&'static str> {
        self.specs().ccc_file
    }

    /// Folder under `Documents/My Games` where the game keeps its INIs
    pub fn my_games_dir(self) -> &'static str {
        self.specs().data_dir_name
    }

    /// Base name of the game's ini files: `<stem>.ini`, `<stem>Custom.ini`, `<stem>Prefs.ini`
    pub fn ini_stem(self) -> &'static str {
        self.specs().ini_stem
    }
}

impl std::fmt::Display for GameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.specs().display_name)
    }
}

/// Returned when a string does not name a game Overseer supports
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown game `{0}` (expected one of: fallout4, skyrimse, starfield)")]
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
    fn my_games_dir_and_ini_stem_match_the_games() {
        // The My Games folder and the INI stem genuinely differ for Skyrim.
        assert_eq!(GameKind::Fallout4.my_games_dir(), "Fallout4");
        assert_eq!(GameKind::Fallout4.ini_stem(), "Fallout4");
        assert_eq!(GameKind::SkyrimSE.my_games_dir(), "Skyrim Special Edition");
        assert_eq!(GameKind::SkyrimSE.ini_stem(), "Skyrim");
        assert_eq!(GameKind::Starfield.my_games_dir(), "Starfield");
        assert_eq!(GameKind::Starfield.ini_stem(), "Starfield");
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
