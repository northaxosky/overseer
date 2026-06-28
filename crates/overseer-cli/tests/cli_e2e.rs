//! End-to-end tests that run the compiled `overseer` binary through real workflows,
//! asserting both its output and the resulting on-disk state.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Run `overseer` with `args` and return the assertion for chaining. `--color never`
/// keeps output plain so string matching is stable.
fn overseer(args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("overseer")
        .unwrap()
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

    // 1. Create the instance (Plugins.txt redirected to a temp dir via --local).
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

    // 2. Stage a mod by hand (an archive isn't needed to exercise mod/deploy commands).
    let mod_file = inst
        .join("mods")
        .join("CoolMod")
        .join("Textures")
        .join("cool.dds");
    std::fs::create_dir_all(mod_file.parent().unwrap()).unwrap();
    std::fs::write(&mod_file, "cool-bytes").unwrap();

    // 3. Enabling reconciles the new mod into the profile, then enables it.
    overseer(&["mod", "enable", "CoolMod", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("CoolMod"));

    // 4. It now appears in the mod list.
    overseer(&["mod", "list", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("CoolMod"));

    // 5. Deploy it.
    overseer(&["deploy", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Deployed"));

    // 6. The file is live under the game's Data/ directory.
    let deployed = game.join("Data").join("Textures").join("cool.dds");
    assert!(deployed.exists(), "mod file should be deployed into Data/");
    assert_eq!(std::fs::read_to_string(&deployed).unwrap(), "cool-bytes");

    // 7. Status reports the live deployment.
    overseer(&["status", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Default"));

    // 8. Purge it.
    overseer(&["purge", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Purged"));

    // 9. The game directory is clean again and status reports nothing.
    assert!(!deployed.exists(), "deployed file removed after purge");
    overseer(&["status", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("No live deployment"));
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

    // init seeds `game` + `script-extender` as ordinary launch targets.
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

    // Bare `launch` lists the available targets.
    overseer(&["launch", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("game").and(predicate::str::contains("script-extender")));

    // Add a real on-disk tool with an argument.
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

    // `exe list` shows it as installed, with its argument.
    overseer(&["exe", "list", "--instance", inst_s])
        .success()
        .stdout(
            predicate::str::contains("FO4Edit")
                .and(predicate::str::contains("installed"))
                .and(predicate::str::contains("-FO4")),
        );

    // Adding the same name again is rejected.
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

    // Removing it drops it from the list (the seeded targets remain).
    overseer(&["exe", "remove", "FO4Edit", "--instance", inst_s])
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

    // `game` is seeded, but its executable isn't on disk.
    overseer(&["launch", "game", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains("not present"));

    // An unknown target name is rejected.
    overseer(&["launch", "bogus", "--instance", inst_s])
        .failure()
        .stderr(predicate::str::contains("no launch target named `bogus`"));
}

#[test]
fn doctor_reports_a_clean_fresh_instance() {
    let tmp = TempDir::new().unwrap();
    let inst = tmp.path().join("inst");
    let inst_s = inst.to_str().unwrap();

    let game = tmp.path().join("game");
    std::fs::create_dir_all(&game).unwrap();
    // A complete install ships its Creation Club manifest in the game root.
    std::fs::write(game.join("Fallout4.ccc"), "ccBGSFO4001-PipBoy(Black).esl\n").unwrap();

    // A controlled INI dir with archive invalidation correctly configured, so the check is
    // clean and deterministic instead of reading the real `My Games\Fallout4`.
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

    // A fresh instance has no plugins, so every count is within limits.
    overseer(&["doctor", "--instance", inst_s])
        .success()
        .stdout(
            predicate::str::contains("Diagnostics: Default")
                .and(predicate::str::contains("0 / 254"))
                .and(predicate::str::contains("No problems found.")),
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

    // Init with both the Plugins.txt dir and the INI dir redirected to temp.
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

    // Stage and enable a mod so there is something to deploy.
    let mod_file = inst
        .join("mods")
        .join("CoolMod")
        .join("Textures")
        .join("cool.dds");
    std::fs::create_dir_all(mod_file.parent().unwrap()).unwrap();
    std::fs::write(&mod_file, "cool-bytes").unwrap();
    overseer(&["mod", "enable", "CoolMod", "--instance", inst_s]).success();

    // Turn per-profile saves on, then the bare form reports the current setting.
    overseer(&["profile", "saves", "on", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("enabled"));
    overseer(&["profile", "saves", "--instance", inst_s])
        .success()
        .stdout(predicate::str::contains("Local saves: on"));

    // Deploying redirects saves into the profile's folder via Fallout4Custom.ini.
    overseer(&["deploy", "--instance", inst_s]).success();
    let custom_ini = my_games.join("Fallout4Custom.ini");
    let written = std::fs::read_to_string(&custom_ini).unwrap();
    assert!(
        written.contains("SLocalSavePath=Saves\\Default\\"),
        "deploy should write the save redirect, got: {written}"
    );

    // Purge removes it (the user had no prior value to restore).
    overseer(&["purge", "--instance", inst_s]).success();
    let after = std::fs::read_to_string(&custom_ini).unwrap_or_default();
    assert!(
        !after.contains("SLocalSavePath"),
        "purge should remove our redirect, got: {after}"
    );
}

/// A minimal BA2: 24-byte header (`BTDX`, `version`, `tag`, count 0, name-table offset 0)
/// plus a body whose bytes a version flip must never touch.
fn ba2_bytes(version: u32, tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"BTDX");
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(tag);
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(body);
    b
}

#[test]
fn patch_ba2_downgrades_a_single_file_and_preserves_the_body() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("Test.ba2");
    let original = ba2_bytes(8, b"GNRL", b"body-must-survive");
    std::fs::write(&file, &original).unwrap();
    let file_s = file.to_str().unwrap();

    // A dry run previews the change but writes nothing.
    overseer(&["patch", "ba2", file_s, "--to", "og", "--dry-run"])
        .success()
        .stdout(predicate::str::contains("would patch v8").and(predicate::str::contains("v1")));
    assert_eq!(
        std::fs::read(&file).unwrap(),
        original,
        "dry run must not write"
    );

    // The real patch flips only the version byte.
    overseer(&["patch", "ba2", file_s, "--to", "og"])
        .success()
        .stdout(predicate::str::contains("patched v8").and(predicate::str::contains("v1")));
    let patched = std::fs::read(&file).unwrap();
    assert_eq!(&patched[4..8], 1u32.to_le_bytes().as_slice());
    assert_eq!(
        &patched[8..],
        &original[8..],
        "everything after the version is untouched"
    );

    // Re-running is idempotent.
    overseer(&["patch", "ba2", file_s, "--to", "og"])
        .success()
        .stdout(predicate::str::contains("already og"));
}

#[test]
fn patch_ba2_directory_requires_yes_then_patches_all() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("Data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("A.ba2"), ba2_bytes(8, b"GNRL", b"aaaa")).unwrap();
    std::fs::write(dir.join("B.ba2"), ba2_bytes(8, b"DX10", b"bbbb")).unwrap();
    // A non-BA2 file in the same dir is simply ignored by the scan.
    std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();
    let dir_s = dir.to_str().unwrap();

    // Without --yes a directory is previewed, not written.
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

    // With --yes both archives are patched.
    overseer(&["patch", "ba2", dir_s, "--to", "og", "--yes"])
        .success()
        .stdout(predicate::str::contains("2 patched"));
    assert_eq!(std::fs::read(dir.join("A.ba2")).unwrap()[4], 1);
    assert_eq!(std::fs::read(dir.join("B.ba2")).unwrap()[4], 1);
}

#[test]
fn patch_ba2_skips_unsupported_and_fails_on_invalid() {
    let tmp = TempDir::new().unwrap();

    // A Starfield-version archive is a benign skip, not an error.
    let sf = tmp.path().join("Starfield.ba2");
    std::fs::write(&sf, ba2_bytes(3, b"GNRL", b"sf")).unwrap();
    overseer(&["patch", "ba2", sf.to_str().unwrap(), "--to", "og"])
        .success()
        .stdout(predicate::str::contains("skipped").and(predicate::str::contains("unsupported")));

    // A file without the BTDX magic is a hard error (non-zero exit).
    let bad = tmp.path().join("Bad.ba2");
    std::fs::write(&bad, b"NOPE-not-a-ba2-header-padding-padding").unwrap();
    overseer(&["patch", "ba2", bad.to_str().unwrap(), "--to", "og"])
        .failure()
        .stdout(predicate::str::contains("BTDX"));
}
