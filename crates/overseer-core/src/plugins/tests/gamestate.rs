//! Tests for plugin game state

use super::*;
use crate::test_support::{FLAG_MASTER, write_plugin};
use tempfile::TempDir;

/// A temp game dir (with `Data/`) + a temp local dir for `Plugins.txt`
fn setup() -> (TempDir, Utf8PathBuf, Utf8PathBuf) {
    let tmp = TempDir::new().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8 path");
    let game = root.join("game");
    let local = root.join("local");
    std::fs::create_dir_all(game.join("Data")).expect("Data dir");
    std::fs::create_dir_all(&local).expect("local dir");
    (tmp, game, local)
}

fn entry(name: &str, active: bool) -> PluginEntry {
    PluginEntry {
        name: name.to_owned(),
        active,
    }
}

#[test]
fn writes_ordered_asterisk_plugins_txt() {
    let (_tmp, game, local) = setup();
    let data = game.join("Data");
    write_plugin(&data, "Aaa.esp", 0, &[]);
    write_plugin(&data, "Bbb.esp", 0, &[]);
    write_plugin(&data, "Ccc.esp", 0, &[]);

    write_active_plugins(
        GameId::Fallout4,
        &game,
        &local,
        &[
            entry("Aaa.esp", true),
            entry("Bbb.esp", false),
            entry("Ccc.esp", true),
        ],
    )
    .expect("write");

    // Order preserved; `*` marks active; inactive listed without a prefix
    let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
    assert_eq!(txt, "*Aaa.esp\nBbb.esp\n*Ccc.esp\n");
}

#[test]
fn masters_serialize_before_normal_plugins() {
    let (_tmp, game, local) = setup();
    let data = game.join("Data");
    write_plugin(&data, "Base.esm", FLAG_MASTER, &[]);
    write_plugin(&data, "Mod.esp", 0, &["Base.esm"]);

    write_active_plugins(
        GameId::Fallout4,
        &game,
        &local,
        &[entry("Base.esm", true), entry("Mod.esp", true)],
    )
    .expect("write");

    let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
    assert_eq!(txt, "*Base.esm\n*Mod.esp\n");
}

#[test]
fn backup_round_trips_raw_bytes() {
    let (_tmp, _game, local) = setup();
    // Nothing written yet
    assert_eq!(read_plugins_txt(&local).expect("read"), None);

    // A Windows-1252 byte (0xE9 = 'é') makes this invalid UTF-8 on purpose
    let original = b"*Caf\xE9.esp\n".to_vec();
    std::fs::write(local.join("Plugins.txt"), &original).expect("seed");

    let backup = read_plugins_txt(&local).expect("read").expect("present");
    assert_eq!(backup, original);

    // Restoring rewrites the exact bytes...
    std::fs::write(local.join("Plugins.txt"), b"clobbered").expect("clobber");
    restore_plugins_txt(&local, Some(&backup)).expect("restore");
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        original
    );

    // ...and restoring `None` removes the file (there was none originally)
    restore_plugins_txt(&local, None).expect("restore none");
    assert!(!local.join("Plugins.txt").exists());
}

#[test]
fn exceeding_the_active_plugin_limit_is_reported() {
    let (_tmp, game, local) = setup();
    let data = game.join("Data");
    let names: Vec<String> = (0..260).map(|i| format!("Mod{i:03}.esp")).collect();
    for name in &names {
        write_plugin(&data, name, 0, &[]);
    }
    let plugins: Vec<PluginEntry> = names.iter().map(|n| entry(n, true)).collect();

    let err = write_active_plugins(GameId::Fallout4, &game, &local, &plugins)
        .expect_err("over the limit");
    assert!(matches!(
        err,
        PluginError::GameState(loadorder::Error::TooManyActivePlugins { .. })
    ));
}

#[test]
fn restore_if_ours_restores_when_the_file_is_untouched() {
    let (_tmp, _game, local) = setup();
    // The live file is still exactly what we wrote
    std::fs::write(local.join("Plugins.txt"), b"*Cool.esp\n").expect("seed current");
    let outcome =
        restore_plugins_txt_if_ours(&local, Some(b"*Original.esp\n"), Some(b"*Cool.esp\n"))
            .expect("restore");
    assert_eq!(outcome, Restore::Restored);
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        b"*Original.esp\n"
    );
}

#[test]
fn restore_if_ours_keeps_a_diverged_file() {
    let (_tmp, _game, local) = setup();
    // The live file no longer matches what we wrote
    std::fs::write(local.join("Plugins.txt"), b"*Edited.esp\n").expect("seed current");
    let outcome =
        restore_plugins_txt_if_ours(&local, Some(b"*Original.esp\n"), Some(b"*Cool.esp\n"))
            .expect("restore");
    assert_eq!(outcome, Restore::Conflict);
    // Left untouched, not rolled back to the original
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        b"*Edited.esp\n"
    );
}

#[test]
fn restore_if_ours_restores_unconditionally_when_nothing_was_written() {
    let (_tmp, _game, local) = setup();
    std::fs::write(local.join("Plugins.txt"), b"*Whatever.esp\n").expect("seed current");
    // intended == None: we never reached the write phase, so fully undo to the original
    let outcome =
        restore_plugins_txt_if_ours(&local, Some(b"*Original.esp\n"), None).expect("restore");
    assert_eq!(outcome, Restore::Restored);
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        b"*Original.esp\n"
    );
}

#[test]
fn implicit_actives_include_hardcoded_masters_and_ccc_entries() {
    let (_tmp, game, local) = setup();
    // A real install ships its Creation Club manifest in the game root
    std::fs::write(
        game.join("Fallout4.ccc"),
        "ccBGSFO4001-PipBoy(Black).esl\nccBGSFO4003-PipBoy(Camo01).esl\n",
    )
    .expect("write ccc");

    let implicit =
        implicit_active_plugins(GameId::Fallout4, &game, &local).expect("implicit actives");

    // The hardcoded base master and a DLC ESM are always candidates, even though no files exist on disk: the set is deliberately not presence-filtered
    assert!(implicit.iter().any(|p| p == "Fallout4.esm"));
    assert!(implicit.iter().any(|p| p == "DLCCoast.esm"));
    // The Creation Club plugins from the manifest are folded in
    assert!(
        implicit
            .iter()
            .any(|p| p == "ccBGSFO4001-PipBoy(Black).esl")
    );
    assert!(
        implicit
            .iter()
            .any(|p| p == "ccBGSFO4003-PipBoy(Camo01).esl")
    );
}
