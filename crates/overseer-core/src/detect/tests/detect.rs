//! Tests for store detection and classification

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

/// A GOG install drops goggame-<id>.info in the game root; detect() reads the store from it
#[test]
fn detect_reads_the_store_from_a_gog_marker_file() {
    use crate::game::GameKind;
    let (_t, game_dir) = temp();
    let gog_id = GameKind::Fallout4.gog_appid().expect("fo4 is on gog");
    std::fs::write(game_dir.join(format!("goggame-{gog_id}.info")), "{}").unwrap();

    let install = detect(GameKind::Fallout4, &game_dir);
    assert_eq!(install.store, Store::Gog);
    assert_eq!(install.version, None);
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
    // An authoritative Steam/GOG manifest wins even when other markers are present
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
    // Microsoft Store is checked before Epic
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
