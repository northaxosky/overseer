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

    overseer(&[
        "instance",
        "init",
        "--path",
        inst_s,
        "--game-dir",
        game.to_str().unwrap(),
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
