//! Detecting which Bethesda game build and store a game directory is

mod fallout4;

pub use fallout4::Edition;

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
        version: read_exe_version(game, game_dir),
    }
}

/// The game specific edition of an install
pub fn edition(install: &GameInstall, game_dir: &Utf8Path) -> Edition {
    match install.game {
        GameKind::Fallout4 => fallout4::classify_edition(install.version, game_dir),
        GameKind::SkyrimSE => todo!("Skyrim SE edition detection"),
        GameKind::Starfield => todo!("Starfield edition detection"),
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

/// `.../steamapps/common/<Game>` => `.../steampps/appmanifest_<id>.acf` two levels up
fn steam_appmanifest_exists(game_dir: &Utf8Path, appid: u32) -> bool {
    game_dir
        .parent()
        .and_then(Utf8Path::parent)
        .is_some_and(|steamapps| steamapps.join(format!("appmanifest_{appid}.acf")).exists())
}

/// Read the game's executable's 4 part PE file version
fn read_exe_version(game: GameKind, game_dir: &Utf8Path) -> Option<ExeVersion> {
    let bytes = std::fs::read(game_dir.join(game.executable())).ok()?;
    pe_file_version(&bytes)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp;

    fn markers(steam: bool, gog: bool, ms: bool, epic: bool) -> StoreMarkers {
        StoreMarkers {
            steam_appmanifest: steam,
            gog_info: gog,
            egstore: epic,
            ms_appxmanifest: ms,
        }
    }

    #[test]
    fn steam_and_gog_markers_classify_their_stores() {
        assert_eq!(
            classify_store(markers(true, false, false, false)),
            Store::Steam
        );
        assert_eq!(
            classify_store(markers(false, true, false, false)),
            Store::Gog
        );
    }

    #[test]
    fn both_steam_and_gog_markers_conflict() {
        assert_eq!(
            classify_store(markers(true, true, false, false)),
            Store::Conflicting
        );
    }

    #[test]
    fn steam_or_gog_win_over_ms_and_epic() {
        // An authoritative Steam/GOG manifest wins even when other markers are present.
        assert_eq!(
            classify_store(markers(true, false, true, true)),
            Store::Steam
        );
        assert_eq!(classify_store(markers(false, true, true, true)), Store::Gog);
    }

    #[test]
    fn ms_store_and_epic_fall_through() {
        assert_eq!(
            classify_store(markers(false, false, true, false)),
            Store::MicrosoftStore
        );
        assert_eq!(
            classify_store(markers(false, false, false, true)),
            Store::Epic
        );
        // Microsoft Store is checked before Epic.
        assert_eq!(
            classify_store(markers(false, false, true, true)),
            Store::MicrosoftStore
        );
    }

    #[test]
    fn no_markers_is_unknown() {
        assert_eq!(
            classify_store(markers(false, false, false, false)),
            Store::Unknown
        );
    }

    #[test]
    fn steam_manifest_is_found_two_levels_up() {
        // <root>/steamapps/common/Fallout 4  ->  <root>/steamapps/appmanifest_377160.acf
        let (_t, root) = temp();
        let steamapps = root.join("steamapps");
        let game_dir = steamapps.join("common").join("Fallout 4");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(steamapps.join("appmanifest_377160.acf"), "").unwrap();

        assert!(steam_appmanifest_exists(&game_dir, 377160));
        assert!(!steam_appmanifest_exists(&game_dir, 489830)); // a different game's appid
    }
}
