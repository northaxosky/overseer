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

/// The per-game constants, declared once per variant so adding a game touches one place
struct GameSpecs {
    load_order_id: GameId,
    executable: &'static str,
    script_extender_loader: &'static str,
    /// Folder name under both `%LOCALAPPDATA%` and `Documents/My Games` (identical per game)
    data_dir_name: &'static str,
    ini_stem: &'static str,
    ccc_file: Option<&'static str>,
    display_name: &'static str,
    /// Steam application id, used to find `steamapps/appmanifest<id>.acf`
    steam_appid: u32,
    /// GOG application id (`goggame-<id>.info`), if the game is on GOG
    gog_appid: Option<u32>,
}

impl GameKind {
    /// All per-game specifics in one place; every accessor below reads a field from here
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
                steam_appid: 377160,
                gog_appid: Some(1998527297),
            },
            Self::SkyrimSE => GameSpecs {
                load_order_id: GameId::SkyrimSE,
                executable: "SkyrimSE.exe",
                script_extender_loader: "skse64_loader.exe",
                data_dir_name: "Skyrim Special Edition",
                ini_stem: "Skyrim",
                ccc_file: Some("Skyrim.ccc"),
                display_name: "Skyrim Special Edition",
                steam_appid: 489830,
                gog_appid: Some(1207658944),
            },
            Self::Starfield => GameSpecs {
                load_order_id: GameId::Starfield,
                executable: "Starfield.exe",
                script_extender_loader: "sfse_loader.exe",
                data_dir_name: "Starfield",
                ini_stem: "Starfield",
                ccc_file: None,
                display_name: "Starfield",
                steam_appid: 1716740,
                gog_appid: None,
            },
        }
    }

    /// The LOOT stack's id, for load order rules (`libloadorder`)
    pub fn load_order_id(self) -> GameId {
        self.specs().load_order_id
    }

    /// The plugin parser id (`esplugin`), from the load order id
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

    /// Steam application id, for location the Steam install manifest
    pub fn steam_appid(self) -> u32 {
        self.specs().steam_appid
    }

    /// GOG application id, if the game is on GOG
    pub fn gog_appid(self) -> Option<u32> {
        self.specs().gog_appid
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

#[cfg(test)]
#[path = "tests/game.rs"]
mod tests;
