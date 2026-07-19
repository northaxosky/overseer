//! End-to-end tests that run the compiled `overseer` binary through real workflows,
//! asserting both its output and the resulting on-disk state.

use assert_cmd::Command;
use camino::Utf8Path;
use overseer_core::test_support::{FLAG_MASTER, tes4_bytes, write_zip};
use overseer_core::{instance::Instance, launch};
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

/// Run `overseer` with `args`; `--color never` keeps output plain for stable assertions
fn overseer(args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("overseer")
        .unwrap()
        .arg("--color")
        .arg("never")
        .args(args)
        .assert()
}

fn overseer_at(current_dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("overseer")
        .unwrap()
        .current_dir(current_dir)
        .arg("--color")
        .arg("never")
        .args(args)
        .assert()
}

#[test]
fn full_workflow_init_enable_deploy_status_purge() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let inst = root.join("inst");
    let game = root.join("game");
    let local = root.join("local");
    let inst_s = inst.to_str().unwrap();

    // 1. Create the instance (Plugins.txt redirected to a temp dir via --local)
    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        game.to_str().unwrap(),
        "--local",
        local.to_str().unwrap(),
    ])
    .success()
    .stdout(predicate::str::contains("Created instance"));

    // 2. Stage a mod by hand (an archive isn't needed to exercise mod/deploy commands)
    let mod_file = inst
        .join("mods")
        .join("CoolMod")
        .join("Textures")
        .join("cool.dds");
    std::fs::create_dir_all(mod_file.parent().unwrap()).unwrap();
    std::fs::write(&mod_file, "cool-bytes").unwrap();

    // 3. Enabling reconciles the new mod into the profile, then enables it
    overseer(&["mod", "enable", "CoolMod", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("CoolMod"));

    // 4. It now appears in the mod list
    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("CoolMod"));

    // 5. Deploy it
    overseer(&["deploy", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Deployed"));

    // 6. The file is live under the game's Data/ directory
    let deployed = game.join("Data").join("Textures").join("cool.dds");
    assert!(deployed.exists(), "mod file should be deployed into Data/");
    assert_eq!(std::fs::read_to_string(&deployed).unwrap(), "cool-bytes");

    // 7. Status reports the live deployment
    overseer(&["status", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Default"));

    // 8. Purge it
    overseer(&["purge", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Purged"));

    // 9. The game directory is clean again and status reports nothing
    assert!(!deployed.exists(), "deployed file removed after purge");
    overseer(&["status", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("No live deployment"));
}

#[test]
fn launch_clear_removes_a_stale_marker() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();
    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        tmp.path().join("game").to_str().unwrap(),
        "--local",
        tmp.path().join("local").to_str().unwrap(),
    ])
    .success();
    let instance = Instance::load(camino::Utf8PathBuf::from(inst_s)).expect("instance");
    let marker = launch::launch_marker_path(&instance);
    std::fs::create_dir_all(marker.parent().expect("marker parent")).expect("marker parent");
    std::fs::write(&marker, b"active").expect("marker");

    overseer(&["launch", "--clear", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Cleared stale launch marker"));

    assert!(!marker.exists());
}

#[test]
fn mod_list_and_move_use_item_ordinals_with_separator_rows() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();
    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        tmp.path().join("game").to_str().unwrap(),
        "--local",
        tmp.path().join("local").to_str().unwrap(),
    ])
    .success();
    for name in ["A", "B", "C"] {
        std::fs::create_dir_all(inst.join("mods").join(name)).unwrap();
    }
    let modlist = inst.join("profiles").join("Default").join("modlist.txt");
    std::fs::write(&modlist, "+A\n|\tseparator\tGameplay\n+B\n+C\n").unwrap();

    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("3 mods"))
        .stdout(predicate::str::contains("1. [x] A"))
        .stdout(predicate::str::contains("2. [x] B"))
        .stdout(predicate::str::contains("3. [x] C"))
        .stdout(predicate::str::contains("Gameplay").not());

    overseer(&["mod", "move", "A", "--to", "2", "--instance", inst_s]).success();

    assert_eq!(
        std::fs::read_to_string(&modlist).unwrap(),
        "|\tseparator\tGameplay\n+B\n+A\n+C\n"
    );
    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("1. [x] B"))
        .stdout(predicate::str::contains("2. [x] A"))
        .stdout(predicate::str::contains("3. [x] C"));
}

#[test]
fn show_on_a_missing_instance_fails() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("nope");
    overseer(&["instance", "show", "--path", missing.to_str().unwrap()])
        .failure()
        .stderr(predicate::str::contains("instance"));
}

#[test]
fn an_invalid_game_is_rejected() {
    let tmp = TempDir::new().unwrap();
    overseer(&[
        "instance",
        "init",
        "--path",
        tmp.path().join("inst").to_str().unwrap(),
        "--game-dir",
        tmp.path().join("game").to_str().unwrap(),
        "--game",
        "morrowind",
    ])
    .failure()
    .stderr(predicate::str::contains("unknown game"));
}

#[test]
fn exe_and_launch_manage_and_list_targets() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let inst = root.join("inst");
    let inst_s = inst.to_str().unwrap();

    // init reports the two derived launch targets
    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        root.join("game").to_str().unwrap(),
    ])
    .success()
    .stdout(predicate::str::contains("targets:").and(predicate::str::contains("script-extender")));

    // Bare `launch` lists the available targets
    overseer(&["launch", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("game").and(predicate::str::contains("script-extender")));

    // Add a real on-disk tool with an argument
    let tool = root.join("FO4Edit.exe");
    std::fs::write(&tool, "").unwrap();
    overseer(&[
        "exe",
        "add",
        "--name",
        "FO4Edit",
        "--path",
        tool.to_str().unwrap(),
        "--arg",
        "-FO4",
        "--instance",
        inst_s,
    ])
    .success()
    .stdout(predicate::str::contains("Added launch target `FO4Edit`"));

    // `exe list` shows derived and user tools with keys, availability, and args
    overseer(&["exe", "list", "--instance", inst_s])
        .success()
        .stdout(
            predicate::str::contains("FO4Edit")
                .and(predicate::str::contains("fo4edit"))
                .and(predicate::str::contains("ready"))
                .and(predicate::str::contains("-FO4")),
        );

    // Adding the same name again is rejected
    overseer(&[
        "exe",
        "add",
        "--name",
        "FO4Edit",
        "--path",
        tool.to_str().unwrap(),
        "--instance",
        inst_s,
    ])
    .failure()
    .stderr(predicate::str::contains("already exists"));

    overseer(&["exe", "remove", "game", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains("cannot be removed"));

    // Removing by stable key drops the user tool while derived targets remain
    overseer(&["exe", "remove", "fo4edit", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Removed launch target `FO4Edit`"));
    overseer(&["exe", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("FO4Edit").not());
}

#[test]
fn launch_reports_missing_and_unknown_targets() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let inst = root.join("inst");
    let inst_s = inst.to_str().unwrap();

    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        root.join("game").to_str().unwrap(),
    ])
    .success();

    // `game` is derived, but its executable isn't on disk
    overseer(&["launch", "game", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains("program is missing"));

    // An unknown target name is rejected
    overseer(&["launch", "bogus", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains("no launch target named `bogus`"));
}

#[test]
fn doctor_reports_a_fresh_instance_with_a_bare_game_dir() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();

    let game = tmp.path().join("game");
    std::fs::create_dir_all(&game).unwrap();
    // A complete install ships its Creation Club manifest in the game root
    std::fs::write(game.join("Fallout4.ccc"), "ccBGSFO4001-PipBoy(Black).esl\n").unwrap();

    // A controlled INI dir with archive invalidation correctly configured, so the check is; clean and deterministic instead of reading the real `My Games\Fallout4`
    let ini = tmp.path().join("ini");
    std::fs::create_dir_all(&ini).unwrap();
    std::fs::write(
        ini.join("Fallout4Custom.ini"),
        "[Archive]\nbInvalidateOlderFiles=1\nsResourceDataDirsFinal=\n",
    )
    .unwrap();

    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        game.to_str().unwrap(),
        "--ini-dir",
        ini.to_str().unwrap(),
    ])
    .success();

    // A fresh instance has no plugins, so every count is within limits; the bare game dir ships
    // no base archives, so the archives check flags that (the complete-install path is covered by
    // the env-gated testbed harness, not this hermetic test)
    overseer(&["doctor", "--instance", inst_s])
        .success()
        .stdout(
            predicate::str::contains("Diagnostics: Default")
                .and(predicate::str::contains("0 / 254"))
                .and(predicate::str::contains("No BA2 archives are loaded"))
                .and(predicate::str::contains("1 warning, 0 errors")),
        );
}

#[test]
fn profile_saves_toggle_redirects_saves_on_deploy() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let inst = root.join("inst");
    let game = root.join("game");
    let local = root.join("local");
    let my_games = root.join("my_games");
    let inst_s = inst.to_str().unwrap();

    // Init with both the Plugins.txt dir and the INI dir redirected to temp
    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        game.to_str().unwrap(),
        "--local",
        local.to_str().unwrap(),
        "--ini-dir",
        my_games.to_str().unwrap(),
    ])
    .success();

    // Stage and enable a mod so there is something to deploy
    let mod_file = inst
        .join("mods")
        .join("CoolMod")
        .join("Textures")
        .join("cool.dds");
    std::fs::create_dir_all(mod_file.parent().unwrap()).unwrap();
    std::fs::write(&mod_file, "cool-bytes").unwrap();
    overseer(&["mod", "enable", "CoolMod", "--instance", inst_s]).success();

    // Turn per-profile saves on, then the bare form reports the current setting
    overseer(&["profile", "saves", "on", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("enabled"));
    overseer(&["profile", "saves", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Local saves: on"));

    // Deploying redirects saves into the profile's folder via Fallout4Custom.ini
    overseer(&["deploy", "--instance", inst_s]).success();
    let custom_ini = my_games.join("Fallout4Custom.ini");
    let written = std::fs::read_to_string(&custom_ini).unwrap();
    assert!(
        written.contains("SLocalSavePath=Saves\\Default\\"),
        "deploy should write the save redirect, got: {written}"
    );

    // Purge removes it (the user had no prior value to restore)
    overseer(&["purge", "--instance", inst_s]).success();
    let after = std::fs::read_to_string(&custom_ini).unwrap_or_default();
    assert!(
        !after.contains("SLocalSavePath"),
        "purge should remove our redirect, got: {after}"
    );
}

/// A minimal BA2 the patch tests flip the version byte on. Reuses the shared core builder
use overseer_core::test_support::ba2_bytes;

#[test]
fn patch_ba2_downgrades_a_single_file_and_preserves_the_body() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("Test.ba2");
    let original = ba2_bytes(8, b"GNRL", b"body-must-survive");
    std::fs::write(&file, &original).unwrap();
    let file_s = file.to_str().unwrap();

    // A dry run previews the change but writes nothing
    overseer(&["patch", "ba2", file_s, "--to", "og", "--dry-run"])
        .success()
        .stdout(predicate::str::contains("would patch v8").and(predicate::str::contains("v1")));
    assert_eq!(
        std::fs::read(&file).unwrap(),
        original,
        "dry run must not write"
    );

    // The real patch (with --yes) flips only the version byte
    overseer(&["patch", "ba2", file_s, "--to", "og", "--yes"])
        .success()
        .stdout(predicate::str::contains("patched v8").and(predicate::str::contains("v1")));
    let patched = std::fs::read(&file).unwrap();
    assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
    assert_eq!(
        &patched[8..],
        &original[8..],
        "everything after the version is untouched"
    );

    // Re-running is idempotent
    overseer(&["patch", "ba2", file_s, "--to", "og"])
        .success()
        .stdout(predicate::str::contains("already og"));
}

#[test]
fn patch_ba2_upgrades_a_single_file_to_ae() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("Test.ba2");
    let original = ba2_bytes(1, b"GNRL", b"body-must-survive");
    std::fs::write(&file, &original).unwrap();
    let file_s = file.to_str().unwrap();

    overseer(&["patch", "ba2", file_s, "--to", "ae", "--yes"])
        .success()
        .stdout(predicate::str::contains("patched v1").and(predicate::str::contains("v8")));
    let patched = std::fs::read(&file).unwrap();
    assert_eq!(&patched[4..8], 8u32.to_le_bytes().as_slice());
    assert_eq!(
        &patched[8..],
        &original[8..],
        "everything after the version is untouched"
    );
}

#[test]
fn patch_ba2_directory_requires_yes_then_patches_all() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("Data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("A.ba2"), ba2_bytes(8, b"GNRL", b"aaaa")).unwrap();
    std::fs::write(dir.join("B.ba2"), ba2_bytes(8, b"DX10", b"bbbb")).unwrap();
    // A non-BA2 file in the same dir is simply ignored by the scan
    std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();
    let dir_s = dir.to_str().unwrap();

    // Without --yes a directory is previewed, not written
    overseer(&["patch", "ba2", dir_s, "--to", "og"])
        .success()
        .stdout(
            predicate::str::contains("re-run with --yes")
                .and(predicate::str::contains("2 patched")),
        );
    assert_eq!(
        std::fs::read(dir.join("A.ba2")).unwrap()[4],
        8,
        "preview must not write"
    );

    // With --yes both archives are patched
    overseer(&["patch", "ba2", dir_s, "--to", "og", "--yes"])
        .success()
        .stdout(predicate::str::contains("2 patched"));
    assert_eq!(std::fs::read(dir.join("A.ba2")).unwrap()[4], 1);
    assert_eq!(std::fs::read(dir.join("B.ba2")).unwrap()[4], 1);
}

#[test]
fn patch_ba2_skips_unsupported_and_fails_on_invalid() {
    let tmp = TempDir::new().unwrap();

    // A Starfield-version archive is a benign skip, not an error
    let sf = tmp.path().join("Starfield.ba2");
    std::fs::write(&sf, ba2_bytes(3, b"GNRL", b"sf")).unwrap();
    overseer(&["patch", "ba2", sf.to_str().unwrap(), "--to", "og"])
        .success()
        .stdout(predicate::str::contains("skipped").and(predicate::str::contains("unsupported")));

    // A file without the BTDX magic is a hard error (non-zero exit)
    let bad = tmp.path().join("Bad.ba2");
    std::fs::write(&bad, b"NOPE-not-a-ba2-header-padding-padding").unwrap();
    overseer(&["patch", "ba2", bad.to_str().unwrap(), "--to", "og"])
        .failure()
        .stdout(predicate::str::contains("BTDX"));
}

#[test]
fn profile_new_creates_a_profile_directory() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();

    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        tmp.path().join("game").to_str().unwrap(),
        "--local",
        tmp.path().join("local").to_str().unwrap(),
    ])
    .success();

    overseer(&["profile", "new", "Hardcore", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Created profile"));

    assert!(
        inst.join("profiles").join("Hardcore").is_dir(),
        "the new profile directory should exist"
    );
}

#[test]
fn mod_rename_renames_the_installed_mod() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();

    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        tmp.path().join("game").to_str().unwrap(),
        "--local",
        tmp.path().join("local").to_str().unwrap(),
    ])
    .success();

    // Stage + enable a mod so it lands in the Default profile's modlist
    let mod_file = inst.join("mods").join("CoolMod").join("readme.txt");
    std::fs::create_dir_all(mod_file.parent().unwrap()).unwrap();
    std::fs::write(&mod_file, "hi").unwrap();
    overseer(&["mod", "enable", "CoolMod", "--instance", inst_s]).success();

    overseer(&[
        "mod",
        "rename",
        "CoolMod",
        "BetterMod",
        "--instance",
        inst_s,
    ])
    .success()
    .stdout(predicate::str::contains("Renamed"));

    // The folder is renamed on disk and the modlist follows
    assert!(inst.join("mods").join("BetterMod").is_dir());
    assert!(!inst.join("mods").join("CoolMod").exists());
    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("BetterMod"));
}

/// Create a temp instance with every path under `tmp` (saves resolve to `<tmp>/ini/Saves/<profile>`)
fn init_instance(tmp: &TempDir) -> std::path::PathBuf {
    init_instance_with_profile(tmp, "Default")
}

fn init_instance_with_profile(tmp: &TempDir, profile: &str) -> std::path::PathBuf {
    let root = tmp.path();
    let inst = root.join("inst");
    overseer(&[
        "instance",
        "init",
        "--path",
        inst.to_str().unwrap(),
        "--game-dir",
        root.join("game").to_str().unwrap(),
        "--local",
        root.join("local").to_str().unwrap(),
        "--ini-dir",
        root.join("ini").to_str().unwrap(),
        "--profile",
        profile,
    ])
    .success();
    inst
}

fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let path = Utf8Path::from_path(path).expect("utf8 archive path");
    write_zip(path, entries);
}

fn install_archive(inst: &Path, archive: &Path, name: &str) -> assert_cmd::assert::Assert {
    let archive_name = archive.file_name().expect("archive basename");
    let destination = inst.join("downloads").join(archive_name);
    if archive != destination {
        std::fs::create_dir_all(destination.parent().expect("downloads parent"))
            .expect("create downloads");
        std::fs::copy(archive, &destination).expect("place archive in Downloads");
    }
    overseer(&[
        "install",
        archive_name.to_str().unwrap(),
        "--name",
        name,
        "--instance",
        inst.to_str().unwrap(),
    ])
}

fn bytes(path: impl AsRef<Path>) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

fn text(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn conflicts_lists_files_provided_by_two_mods() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();

    // Two enabled mods provide the same file
    for m in ["AlphaMod", "BetaMod"] {
        let f = inst
            .join("mods")
            .join(m)
            .join("Textures")
            .join("shared.dds");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, m).unwrap();
        overseer(&["mod", "enable", m, "--instance", inst_s]).success();
    }

    overseer(&["conflicts", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("shared.dds"))
        .stdout(predicate::str::contains("AlphaMod"))
        .stdout(predicate::str::contains("BetaMod"));
}

#[test]
fn conflicts_reports_none_for_a_clean_profile() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    overseer(&["conflicts", "--instance", inst.to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("No file conflicts"));
}

#[test]
fn downloads_lists_installable_archives() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let dl = inst.join("downloads");
    std::fs::create_dir_all(&dl).unwrap();
    std::fs::write(dl.join("CoolMod.7z"), b"not-a-real-archive").unwrap();

    overseer(&["downloads", "--instance", inst.to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("CoolMod.7z"));
}

#[test]
fn install_uses_a_downloads_basename_and_reconciles_lazily() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let inst = init_instance_with_profile(&tmp, "Survival");
    let inst_s = inst.to_str().unwrap();
    let modlist = inst.join("profiles").join("Survival").join("modlist.txt");
    std::fs::write(&modlist, b"+Existing\r\n").unwrap();
    std::fs::create_dir_all(inst.join("mods/Existing")).unwrap();

    let archive = inst.join("downloads/TexturePack.zip");
    zip(
        &archive,
        &[("Data/Textures/installed.dds", b"installed-bytes")],
    );
    let archive_bytes = bytes(&archive);

    overseer_at(root, &["install", "TexturePack.zip", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains(
            "Installed `TexturePack` from `TexturePack.zip`",
        ))
        .stdout(predicate::str::contains("re-run with --yes").not());

    assert_eq!(
        text(inst.join("mods/TexturePack/Textures/installed.dds")),
        "installed-bytes"
    );
    assert_eq!(text(&modlist), "+Existing\r\n");
    assert_eq!(bytes(&archive), archive_bytes);
    assert!(!inst.join("mods/TexturePack/.overseer-mod.toml").exists());

    overseer(&["mod", "list", "--instance", inst_s, "--profile", "Survival"])
        .success()
        .stdout(predicate::str::contains("TexturePack"));
    // `mod list` is a read: it reconciles in memory and must not rewrite modlist.txt
    assert_eq!(text(modlist), "+Existing\r\n");
}

#[test]
fn commands_without_a_profile_flag_use_the_configured_default() {
    let tmp = TempDir::new().unwrap();
    // Init makes "Survival" the default profile (and the only one that exists)
    let inst = init_instance_with_profile(&tmp, "Survival");
    let inst_s = inst.to_str().unwrap();

    // No --profile: resolves to config.default_profile ("Survival"), not a hardcoded "Default"
    overseer(&["profile", "saves", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("profile `Survival`"));

    // An explicit --profile still takes precedence
    overseer(&["mod", "list", "--instance", inst_s, "--profile", "Survival"]).success();
}

#[test]
fn mod_list_does_not_persist_a_transiently_missing_mod() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    // Two enabled mods in the profile, but one folder is temporarily absent
    // (external drive, cloud sync, mid-operation) so reconcile would drop it
    let modlist = inst.join("profiles/Default/modlist.txt");
    std::fs::write(&modlist, b"+Present\r\n+Missing\r\n").unwrap();
    std::fs::create_dir_all(inst.join("mods/Present")).unwrap();
    let before = bytes(&modlist);

    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Present"));

    // A read must never persist the drop, or the mod's enabled state and priority are lost
    assert_eq!(bytes(&modlist), before);
}

#[test]
fn mod_remove_leaves_profiles_for_lazy_reconciliation() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    let archive = inst.join("downloads/Cool.zip");
    zip(&archive, &[("Textures/file.dds", b"installed")]);
    install_archive(&inst, &archive, "CoolMod").success();
    overseer(&["profile", "new", "Alternate", "--instance", inst_s]).success();
    let default = inst.join("profiles/Default/modlist.txt");
    let alternate = inst.join("profiles/Alternate/modlist.txt");
    std::fs::write(&default, b"+CoolMod\r\n+Keep\r\n").unwrap();
    std::fs::write(&alternate, b"-Other\n-CoolMod\n").unwrap();
    std::fs::create_dir_all(inst.join("mods/Keep")).unwrap();
    std::fs::create_dir_all(inst.join("mods/Other")).unwrap();
    let before = [bytes(&default), bytes(&alternate)];
    let download = bytes(&archive);

    overseer(&["mod", "remove", "coolmod", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Removed `CoolMod`"))
        .stdout(predicate::str::contains("re-run with --yes").not());

    assert!(!inst.join("mods/CoolMod").exists());
    assert_eq!([bytes(&default), bytes(&alternate)], before);
    assert_eq!(bytes(&archive), download);

    overseer(&["mod", "list", "--instance", inst_s]).success();
    overseer(&[
        "mod",
        "list",
        "--instance",
        inst_s,
        "--profile",
        "Alternate",
    ])
    .success();
    // Reads reconcile in memory only, so the modlists are still exactly what `mod remove` left
    assert_eq!([bytes(&default), bytes(&alternate)], before);
}

#[test]
fn mod_replace_preserves_profiles_without_writing_reserved_metadata() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    let original = inst.join("downloads/Original.zip");
    let replacement = inst.join("downloads/Replacement.zip");
    zip(&original, &[("Textures/file.dds", b"original")]);
    zip(&replacement, &[("Textures/file.dds", b"replacement")]);
    install_archive(&inst, &original, "CoolMod").success();
    overseer(&["profile", "new", "Alternate", "--instance", inst_s]).success();
    let default = inst.join("profiles/Default/modlist.txt");
    let alternate = inst.join("profiles/Alternate/modlist.txt");
    std::fs::write(&default, b"# exact\r\n+CoolMod").unwrap();
    std::fs::write(&alternate, b"-CoolMod\r\n").unwrap();
    let before = [bytes(&default), bytes(&alternate)];

    overseer(&[
        "mod",
        "replace",
        "coolmod",
        "Replacement.zip",
        "--instance",
        inst_s,
    ])
    .success()
    .stdout(predicate::str::contains(
        "Replaced `CoolMod` from `Replacement.zip`",
    ));
    assert_eq!(
        text(inst.join("mods/CoolMod/Textures/file.dds")),
        "replacement"
    );
    assert!(!inst.join("mods/CoolMod/.overseer-mod.toml").exists());
    assert_eq!([bytes(&default), bytes(&alternate)], before);
}

#[test]
fn lifecycle_guards_do_not_mutate_installed_mods_or_profiles() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    let archive = inst.join("downloads/Guarded.zip");
    let replacement = inst.join("downloads/Replacement.zip");
    zip(&archive, &[("Textures/file.dds", b"guarded")]);
    zip(&replacement, &[("Textures/file.dds", b"replacement")]);
    install_archive(&inst, &archive, "Guarded").success();
    let live = inst.join("mods/Guarded/Textures/file.dds");
    let modlist = inst.join("profiles/Default/modlist.txt");
    let original_modlist = bytes(&modlist);

    let pending = inst.join("state").join("pending-mod-operation");
    std::fs::create_dir_all(&pending).unwrap();
    std::fs::write(pending.join("manual.txt"), b"manual").unwrap();
    overseer(&["mod", "remove", "Guarded", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains(pending.to_str().unwrap()))
        .stderr(predicate::str::contains("pending mod operation"));
    assert_eq!(text(&live), "guarded");
    assert_eq!(bytes(&modlist), original_modlist);
    assert_eq!(text(pending.join("manual.txt")), "manual");
    std::fs::remove_dir_all(pending).unwrap();

    overseer(&["mod", "enable", "Guarded", "--instance", inst_s]).success();
    overseer(&["deploy", "--instance", inst_s]).success();
    let deployed_modlist = bytes(&modlist);
    overseer(&[
        "mod",
        "replace",
        "Guarded",
        "Replacement.zip",
        "--instance",
        inst_s,
    ])
    .failure()
    .stderr(predicate::str::contains("deployment state exists"))
    .stderr(predicate::str::contains("deployment.json"))
    .stderr(predicate::str::contains("purge it first"));
    assert_eq!(text(&live), "guarded");
    assert_eq!(bytes(&modlist), deployed_modlist);
    assert!(replacement.exists());

    overseer(&["purge", "--instance", inst_s]).success();
}

#[test]
fn install_reports_duplicates_and_fomod_archives() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    let plain = inst.join("downloads/Plain.zip");
    let scripted = inst.join("downloads/Scripted.zip");
    zip(&plain, &[("Textures/a.dds", b"x")]);
    zip(
        &scripted,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"x"),
        ],
    );

    overseer(&[
        "install",
        "Plain.zip",
        "--name",
        "Plain",
        "--instance",
        inst_s,
    ])
    .success();
    overseer(&[
        "install",
        "Plain.zip",
        "--name",
        "Plain",
        "--instance",
        inst_s,
    ])
    .failure()
    .stderr(predicate::str::contains("already exists"));

    overseer(&[
        "install",
        "Scripted.zip",
        "--name",
        "Scripted",
        "--instance",
        inst_s,
    ])
    .failure()
    .stderr(predicate::str::contains("FOMOD installers"));
    assert!(!inst.join("mods").join("Scripted").exists());
}

#[test]
fn plugin_list_activate_deactivate_and_deploy_updates_plugins_txt() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();
    let mods = inst.join("mods");
    let base = mods.join("BaseMod");
    let patch = mods.join("PatchMod");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&patch).unwrap();
    std::fs::write(base.join("Base.esm"), tes4_bytes(FLAG_MASTER, &[])).unwrap();
    std::fs::write(patch.join("Patch.esp"), tes4_bytes(0, &["Base.esm"])).unwrap();

    overseer(&["mod", "enable", "PatchMod", "--instance", inst_s]).success();
    overseer(&["mod", "enable", "BaseMod", "--instance", inst_s]).success();

    overseer(&["plugin", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Base.esm").and(predicate::str::contains("master")))
        .stdout(predicate::str::contains("Patch.esp"));

    overseer(&["plugin", "deactivate", "Patch.esp", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Deactivated `Patch.esp`"));
    let profile_plugins =
        std::fs::read_to_string(inst.join("profiles").join("Default").join("plugins.txt")).unwrap();
    assert!(profile_plugins.contains("Patch.esp\n"));
    assert!(!profile_plugins.contains("*Patch.esp"));

    overseer(&["plugin", "activate", "Patch.esp", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Activated `Patch.esp`"));
    overseer(&["deploy", "--instance", inst_s]).success();

    let live_plugins =
        std::fs::read_to_string(tmp.path().join("local").join("Plugins.txt")).unwrap();
    assert_eq!(live_plugins, "*Base.esm\n*Patch.esp\n");
}

#[test]
fn saves_list_and_delete_manage_fos_files() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();

    // saves_dir resolves to <ini>/Saves/Default; a `.fos` with an unreadable header still lists
    let saves = tmp.path().join("ini").join("Saves").join("Default");
    std::fs::create_dir_all(&saves).unwrap();
    let save = saves.join("Save1_abc.fos");
    std::fs::write(&save, b"not-a-real-save").unwrap();

    overseer(&["saves", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Save1_abc.fos"));

    overseer(&["saves", "delete", "Save1_abc.fos", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Deleted"));
    assert!(!save.exists(), "the .fos should be gone after delete");
}

#[test]
fn saves_delete_refuses_a_path_that_escapes_the_profile() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();

    // A save that lives outside the Default profile's saves dir
    let outside = tmp.path().join("ini").join("Saves").join("Other");
    std::fs::create_dir_all(&outside).unwrap();
    let victim = outside.join("Keep.fos");
    std::fs::write(&victim, b"not-a-real-save").unwrap();

    // `..\Other\Keep.fos` would resolve outside Default/ without the guard
    overseer(&[
        "saves",
        "delete",
        "..\\Other\\Keep.fos",
        "--instance",
        inst_s,
    ])
    .failure()
    .stderr(predicate::str::contains("plain file name"));

    assert!(victim.exists(), "a traversal path must not delete the file");
}

/// Write a one-file GNRL archive at `path`, mirroring how the core merge tests build sources
fn write_main_ba2(path: &Path, entry: &str) {
    let files = [overseer_core::ba2::Ba2File {
        path: entry.to_owned(),
        bytes: b"payload".to_vec(),
    }];
    let img = overseer_core::ba2::pack_general(&files, |_| false).expect("pack general");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, img).unwrap();
}

#[test]
fn merge_cc_dry_run_reports_the_plan_without_writing() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();

    // Two Creation Club plugins, each active with a Main archive in Data/
    let data = tmp.path().join("game").join("Data");
    write_main_ba2(
        &data.join("ccBGSFO4001-PipBoy(Black) - Main.ba2"),
        "black/a.nif",
    );
    write_main_ba2(
        &data.join("ccBGSFO4002-PipBoy(Blue) - Main.ba2"),
        "blue/b.nif",
    );
    std::fs::write(
        inst.join("profiles").join("Default").join("plugins.txt"),
        "*ccBGSFO4001-PipBoy(Black).esl\n*ccBGSFO4002-PipBoy(Blue).esl\n",
    )
    .unwrap();

    overseer(&["merge", "--cc", "--dry-run", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Dry run"))
        .stdout(predicate::str::contains(
            "2 plugin(s), 2 source archive(s) will be merged into `CCMerged`",
        ));

    // A dry run writes nothing: no managed mod, no manifest
    assert!(!inst.join("mods").join("CCMerged").exists());
    assert!(!inst.join("merges").join("CCMerged.json").exists());
}

#[test]
fn merge_list_then_restore_round_trips_the_sources() {
    let tmp = TempDir::new().unwrap();
    let inst = init_instance(&tmp);
    let inst_s = inst.to_str().unwrap();

    let data = tmp.path().join("game").join("Data");
    let a_ba2 = data.join("ModA - Main.ba2");
    let b_ba2 = data.join("ModB - Main.ba2");
    write_main_ba2(&a_ba2, "a/one.nif");
    write_main_ba2(&b_ba2, "b/two.nif");
    std::fs::write(
        inst.join("profiles").join("Default").join("plugins.txt"),
        "*ModA.esp\n*ModB.esp\n",
    )
    .unwrap();

    let list = tmp.path().join("merge-list.txt");
    std::fs::write(&list, "# merge these\nModA.esp\nModB.esp\n").unwrap();

    overseer(&[
        "merge",
        "--list",
        list.to_str().unwrap(),
        "--name",
        "Merged",
        "--yes",
        "--instance",
        inst_s,
    ])
    .success()
    .stdout(predicate::str::contains("Merged into `Merged`"));

    // The managed mod and manifest exist; the sources moved out of Data/
    assert!(inst.join("mods").join("Merged").is_dir());
    assert!(inst.join("merges").join("Merged.json").is_file());
    assert!(!a_ba2.exists());
    assert!(!b_ba2.exists());

    overseer(&["merge", "--restore", "Merged", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Restored merge `Merged`"));

    // Restore returns the sources and drops the mod and manifest
    assert!(a_ba2.exists());
    assert!(b_ba2.exists());
    assert!(!inst.join("mods").join("Merged").exists());
    assert!(!inst.join("merges").join("Merged.json").exists());
}
