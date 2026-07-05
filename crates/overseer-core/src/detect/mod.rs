//! Detecting which Bethesda game build and store a game directory is

mod fallout4;

pub use fallout4::{
    Edition, Generation, address_library_name, loader_family, packed_runtime, runtime_family,
};

use crate::game::GameKind;
use camino::Utf8Path;
use std::fmt;

/// A 4 part PE file version, e.g. `1.10.163.0`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExeVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub build: u16,
}

impl fmt::Display for ExeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.build
        )
    }
}

/// Which storefront the install came from, by marker files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Steam,
    Gog,
    Epic,
    MicrosoftStore,
    /// Both Steam and GOG markers were present
    Conflicting,
    Unknown,
}

/// Game agnostic facts about an install: which game, store, and exe version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameInstall {
    pub game: GameKind,
    pub store: Store,
    pub version: Option<ExeVersion>,
}

/// Gather the game-agnostic facts about the install at `game-dir`
pub fn detect(game: GameKind, game_dir: &Utf8Path) -> GameInstall {
    GameInstall {
        game,
        store: classify_store(probe_store_markers(game, game_dir)),
        version: file_version(&game_dir.join(game.executable())),
    }
}

/// The game specific edition of an install
pub fn edition(install: &GameInstall, game_dir: &Utf8Path) -> Edition {
    match install.game {
        GameKind::Fallout4 => fallout4::classify_edition(install.version, game_dir),
        // Edition detection isn't implemented for these yet; degrade rather than panic in a library
        GameKind::SkyrimSE | GameKind::Starfield => Edition::Undetermined,
    }
}

/// Which store marker files were found near the game directory
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StoreMarkers {
    steam_appmanifest: bool,
    gog_info: bool,
    egstore: bool,
    ms_appxmanifest: bool,
}

/// Map marker files to a [`Store`]
fn classify_store(m: StoreMarkers) -> Store {
    match (m.steam_appmanifest, m.gog_info) {
        (true, true) => Store::Conflicting,
        (true, false) => Store::Steam,
        (false, true) => Store::Gog,
        (false, false) if m.ms_appxmanifest => Store::MicrosoftStore,
        (false, false) if m.egstore => Store::Epic,
        (false, false) => Store::Unknown,
    }
}

fn probe_store_markers(game: GameKind, game_dir: &Utf8Path) -> StoreMarkers {
    StoreMarkers {
        steam_appmanifest: steam_appmanifest_exists(game_dir, game.steam_appid()),
        gog_info: game
            .gog_appid()
            .is_some_and(|id| game_dir.join(format!("goggame-{id}.info")).exists()),
        egstore: game_dir.join(".egstore").is_dir(),
        ms_appxmanifest: game_dir.join("appxmanifest.xml").exists(),
    }
}

/// `.../steamapps/common/<Game>` => `.../steamapps/appmanifest_<id>.acf` two levels up
fn steam_appmanifest_exists(game_dir: &Utf8Path, appid: u32) -> bool {
    game_dir
        .parent()
        .and_then(Utf8Path::parent)
        .is_some_and(|steamapps| steamapps.join(format!("appmanifest_{appid}.acf")).exists())
}

/// The PE file version of any on-disk binary, or `None` if unreadable / version-less
pub fn file_version(path: &Utf8Path) -> Option<ExeVersion> {
    let map = pelite::FileMap::open(path).ok()?;
    pe_file_version(map.as_ref())
}

/// Extract the PE `VS_FIXEDFILEINFO` file version from raw bytes
fn pe_file_version(bytes: &[u8]) -> Option<ExeVersion> {
    use pelite::pe64::{Pe, PeFile};
    let pe = PeFile::from_bytes(bytes).ok()?;
    let v = pe
        .resources()
        .ok()?
        .version_info()
        .ok()?
        .fixed()?
        .dwFileVersion;
    Some(ExeVersion {
        major: v.Major,
        minor: v.Minor,
        patch: v.Patch,
        build: v.Build,
    })
}

#[cfg(test)]
#[path = "tests/detect.rs"]
mod tests;
