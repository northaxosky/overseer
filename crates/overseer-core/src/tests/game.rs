//! Tests for GameKind mappings and serialization

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
    // The My Games folder and the INI stem genuinely differ for Skyrim
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

    // Instances persist this in overseer.toml, so the on-disk form must stay; stable -- a rename here would silently break existing instances
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
