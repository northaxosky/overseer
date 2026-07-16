//! Tests for deploy, purge, and status orchestration

use super::*;
use crate::deploy::NullSink;
use crate::instance::Instance;
use crate::test_support::{install_mod, install_plugin, save_profile, temp_instance};
use camino::Utf8PathBuf;

/// Absolute path of a file as it would land under the game's Data/ directory
fn deployed(instance: &Instance, rel: &str) -> Utf8PathBuf {
    instance.config.game_dir.join("Data").join(rel)
}

/// Rewrite the on-disk journal's status to mimic a crash at a given stage
fn force_status(instance: &Instance, status: Status) {
    let mut deployment = Deployment::load(instance).expect("load journal");
    deployment.status = status;
    deployment.committed = Some(status == Status::Committed);
    deployment.save(instance).expect("save journal");
}

#[test]
fn deploy_hardlinks_enabled_mod_files_into_data() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let path = deployed(&instance, "Textures/a.dds");
    assert!(path.exists(), "file should be deployed into Data/");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "pixels");
}

#[test]
fn deploy_records_recoverable_state() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    assert_eq!(deployment.profile, "Default");
    assert!(Deployment::exists(&instance));
    assert!(Deployment::path(&instance).exists());
    assert!(Deployment::baseline_path(&instance).exists());
    assert!(
        !std::fs::read_to_string(Deployment::path(&instance))
            .expect("journal")
            .contains("preexisting_paths"),
        "the reversal baseline stays in its sidecar"
    );
}

#[test]
fn purge_removes_deployed_files_and_state() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Meshes/m.nif", "tris")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let path = deployed(&instance, "Meshes/m.nif");
    assert!(path.exists());

    purge(&instance, &NullSink).expect("purge");
    assert!(!path.exists(), "purge should remove deployed files");
    assert!(!Deployment::exists(&instance), "purge should clear state");
}

#[test]
fn second_deploy_is_refused() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("first deploy");
    let err = deploy_profile(&instance, "Default", &NullSink).expect_err("second deploy must fail");
    assert!(matches!(err, ApplyError::AlreadyDeployed { .. }));
}

#[test]
fn deploy_missing_profile_is_refused() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);

    let err = deploy_profile(&instance, "Typo", &NullSink).expect_err("missing profile");

    assert!(
        matches!(err, ApplyError::Instance(crate::instance::InstanceError::ProfileNotFound(name)) if name == "Typo")
    );
}

#[test]
fn rename_mod_is_refused_while_a_deployment_is_live() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let err = rename_mod(&instance, "CoolMod", "BetterMod")
        .expect_err("rename must be refused while deployed");
    assert!(matches!(err, ApplyError::DeployedCannotRename { .. }));
    assert!(instance.mods_dir().join("CoolMod").is_dir());
    assert!(!instance.mods_dir().join("BetterMod").exists());
}

#[test]
fn rename_mod_succeeds_when_no_deployment_is_live() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    rename_mod(&instance, "CoolMod", "BetterMod").expect("rename");

    assert!(!instance.mods_dir().join("CoolMod").exists());
    assert!(instance.mods_dir().join("BetterMod").is_dir());
}

#[test]
fn rename_profile_is_refused_while_that_profile_is_deployed() {
    let (_tmp, mut instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let err = rename_profile(&mut instance, "Default", "Main")
        .expect_err("renaming the deployed profile must be refused");
    assert!(matches!(err, ApplyError::DeployedCannotRename { .. }));
    assert!(instance.profile_dir("Default").is_dir());
}

#[test]
fn rename_profile_is_allowed_while_a_different_profile_is_deployed() {
    let (_tmp, mut instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    save_profile(&instance, "Other", &[]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    rename_profile(&mut instance, "Other", "Renamed")
        .expect("renaming a non-deployed profile is allowed");
    assert!(instance.profile_dir("Renamed").is_dir());
    assert!(!instance.profile_dir("Other").exists());
}

#[test]
fn rename_profile_syncs_the_default_pointer() {
    let (_tmp, mut instance) = temp_instance();
    save_profile(&instance, "Default", &[]);
    assert_eq!(instance.config.default_profile, "Default");

    rename_profile(&mut instance, "Default", "Main").expect("rename");

    assert_eq!(instance.config.default_profile, "Main");
    // The change is persisted, so a fresh load sees it too
    let reloaded = Instance::load(&instance.root).expect("reload");
    assert_eq!(reloaded.config.default_profile, "Main");
}

#[test]
fn purge_without_deployment_errors() {
    let (_tmp, instance) = temp_instance();
    let err = purge(&instance, &NullSink).expect_err("purge with nothing deployed must fail");
    assert!(matches!(err, ApplyError::NotDeployed { .. }));
}

#[test]
fn deploy_backs_up_and_purge_restores_a_preexisting_data_file() {
    let (_tmp, instance) = temp_instance();
    // A vanilla file already in the game's Data/ that a mod will overwrite
    let data_file = deployed(&instance, "Textures/conflict.dds");
    std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
    std::fs::write(&data_file, "vanilla").expect("seed vanilla");

    // A mod shipping the same file (non-plugin, so no load-order parsing)
    install_mod(
        &instance,
        "Overwriter",
        &[("Textures/conflict.dds", "modded")],
    );
    save_profile(&instance, "Default", &[("Overwriter", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    // The mod's version wins at the destination
    assert_eq!(std::fs::read_to_string(&data_file).expect("read"), "modded");

    let outcome = purge(&instance, &NullSink).expect("purge");
    // The vanilla original is restored byte-for-byte
    assert_eq!(
        std::fs::read_to_string(&data_file).expect("read"),
        "vanilla"
    );
    assert_eq!(outcome.restored.len(), 1);
}

#[test]
fn purge_captures_a_data_file_replaced_after_deployment() {
    let (_tmp, instance) = temp_instance();
    // A mod ships a non-plugin file we deploy as a hard link
    install_mod(&instance, "Texturer", &[("Textures/a.dds", "ours")]);
    save_profile(&instance, "Default", &[("Texturer", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let dest = deployed(&instance, "Textures/a.dds");
    assert_eq!(std::fs::read_to_string(&dest).expect("read"), "ours");

    // A tool rewrites the deployed file in place after deployment, breaking the link
    std::fs::remove_file(&dest).expect("remove our link");
    std::fs::write(&dest, "tool output").expect("tool writes");

    let outcome = purge(&instance, &NullSink).expect("purge");
    assert!(!dest.exists(), "captured output leaves the game folder");
    assert_eq!(
        std::fs::read_to_string(instance.overwrite_dir().join("Textures/a.dds")).expect("read"),
        "tool output"
    );
    assert_eq!(outcome.captured.len(), 1);
}

#[test]
fn deploy_routes_root_content_to_the_game_root_and_purge_restores() {
    let (_tmp, instance) = temp_instance();
    // A mod with Root/ content (-> game root) and ordinary Data content (-> Data/)
    install_mod(
        &instance,
        "ScriptExtender",
        &[
            ("Root/f4se_loader.exe", "loader"),
            ("Scripts/Quest.pex", "script"),
        ],
    );
    save_profile(&instance, "Default", &[("ScriptExtender", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let root_exe = instance.config.game_dir.join("f4se_loader.exe");
    let data_file = deployed(&instance, "Scripts/Quest.pex");
    assert_eq!(
        std::fs::read_to_string(&root_exe).expect("root file"),
        "loader"
    );
    assert_eq!(
        std::fs::read_to_string(&data_file).expect("data file"),
        "script"
    );

    purge(&instance, &NullSink).expect("purge");
    assert!(!root_exe.exists(), "root file removed on purge");
    assert!(!data_file.exists(), "data file removed on purge");
}

#[test]
fn deploy_backs_up_and_purge_restores_a_preexisting_root_file() {
    let (_tmp, instance) = temp_instance();
    // A vanilla DLL already sitting next to the game exe that a mod overwrites
    std::fs::create_dir_all(&instance.config.game_dir).expect("mk game dir");
    let root_dll = instance.config.game_dir.join("dxgi.dll");
    std::fs::write(&root_dll, "vanilla").expect("seed vanilla");

    install_mod(&instance, "ReShade", &[("Root/dxgi.dll", "modded")]);
    save_profile(&instance, "Default", &[("ReShade", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(std::fs::read_to_string(&root_dll).expect("read"), "modded");

    purge(&instance, &NullSink).expect("purge");
    // The vanilla original next to the exe is restored byte-for-byte
    assert_eq!(std::fs::read_to_string(&root_dll).expect("read"), "vanilla");
}

#[test]
fn purge_captures_a_generated_file_from_a_mod_created_dir() {
    let (_tmp, instance) = temp_instance();
    // A mod whose file forces creating Data/F4SE/Plugins/
    install_mod(
        &instance,
        "Buffout",
        &[("F4SE/Plugins/Buffout4.dll", "plugin")],
    );
    save_profile(&instance, "Default", &[("Buffout", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // The game writes a crash log next to the plugin during play
    let generated = instance
        .config
        .game_dir
        .join("Data/F4SE/Plugins/Buffout4.log");
    std::fs::write(&generated, "crashlog").expect("simulate runtime write");

    purge(&instance, &NullSink).expect("purge");

    // Captured into overwrite/ in staging layout (the Data/ prefix is stripped)
    let captured = instance.overwrite_dir().join("F4SE/Plugins/Buffout4.log");
    assert_eq!(
        std::fs::read_to_string(&captured).expect("captured file"),
        "crashlog"
    );
    // ...and gone from the game dir, which is left clean
    assert!(
        !generated.exists(),
        "generated file moved out of the game dir"
    );
    assert!(
        !instance.config.game_dir.join("Data/F4SE").exists(),
        "emptied created dirs are removed"
    );
}

/// During purge, capture must skip our own deployed files and move only foreign runtime output into overwrite/
#[test]
fn purge_capture_never_vacuums_our_own_deployed_files() {
    let (_tmp, instance) = temp_instance();
    // A mod whose plugin forces creating Data/F4SE/Plugins/
    install_mod(
        &instance,
        "Buffout",
        &[("F4SE/Plugins/Buffout4.dll", "plugin")],
    );
    save_profile(&instance, "Default", &[("Buffout", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // The game drops a log beside our plugin, in the same directory we created
    let generated = instance
        .config
        .game_dir
        .join("Data/F4SE/Plugins/Buffout4.log");
    std::fs::write(&generated, "crashlog").expect("simulate runtime write");

    purge(&instance, &NullSink).expect("purge");

    let overwrite = instance.overwrite_dir();
    assert!(
        overwrite.join("F4SE/Plugins/Buffout4.log").exists(),
        "the generated foreign file is captured"
    );
    assert!(
        !overwrite.join("F4SE/Plugins/Buffout4.dll").exists(),
        "our own deployed file is excluded from capture"
    );
    assert!(
        instance
            .mods_dir()
            .join("Buffout/F4SE/Plugins/Buffout4.dll")
            .exists(),
        "the deployed file's staging source is untouched"
    );
}

/// overwrite_staging_path is the exact inverse of the Root/Data deploy mapping
#[test]
fn overwrite_staging_path_inverts_the_root_data_mapping() {
    // Data/ content sheds the Data/ prefix, back into the mod's staging layout
    assert_eq!(
        overwrite_staging_path(&Utf8Path::new("Data").join("F4SE").join("Buffout4.log")),
        Utf8Path::new("F4SE").join("Buffout4.log")
    );
    // A game-root file (outside Data/) is attributed to the mod's Root/ folder
    assert_eq!(
        overwrite_staging_path(Utf8Path::new("f4se_loader.exe")),
        Utf8Path::new(ROOT_DIR).join("f4se_loader.exe")
    );
    assert_eq!(
        overwrite_staging_path(&Utf8Path::new("enbseries").join("cache.bin")),
        Utf8Path::new("Root").join("enbseries").join("cache.bin")
    );
}

#[test]
fn purge_captures_generated_files_in_preexisting_dirs() {
    let (_tmp, instance) = temp_instance();
    // Data/ already exists, like a real (vanilla) game install
    std::fs::create_dir_all(instance.config.game_dir.join("Data")).expect("vanilla Data");
    install_mod(&instance, "Tex", &[("Textures/x.dds", "pix")]);
    save_profile(&instance, "Default", &[("Tex", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // A tool writes a log directly into the pre-existing Data/ root
    let loose = instance.config.game_dir.join("Data/loose.log");
    std::fs::write(&loose, "log").expect("write");

    let outcome = purge(&instance, &NullSink).expect("purge");

    assert!(!loose.exists(), "baseline-new residue is moved out");
    assert_eq!(
        std::fs::read_to_string(instance.overwrite_dir().join("loose.log")).expect("captured"),
        "log"
    );
    assert_eq!(outcome.captured.len(), 1);
}

#[test]
fn captured_files_redeploy_into_the_game_dir() {
    let (_tmp, instance) = temp_instance();
    install_mod(
        &instance,
        "Buffout",
        &[("F4SE/Plugins/Buffout4.dll", "plugin")],
    );
    save_profile(&instance, "Default", &[("Buffout", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy 1");

    let generated = instance
        .config
        .game_dir
        .join("Data/F4SE/Plugins/Buffout4.log");
    std::fs::write(&generated, "crashlog").expect("write");
    purge(&instance, &NullSink).expect("purge 1");
    assert!(!generated.exists());

    // Re-deploy: the captured file comes back to the same game-dir location
    deploy_profile(&instance, "Default", &NullSink).expect("deploy 2");
    assert_eq!(
        std::fs::read_to_string(&generated).expect("redeployed"),
        "crashlog"
    );
}

#[test]
fn purge_captures_a_generated_file_in_a_mod_created_root_dir() {
    let (_tmp, instance) = temp_instance();
    // A Root mod that introduces enbseries/ in the game root
    install_mod(&instance, "ENB", &[("Root/enbseries/enbseries.ini", "cfg")]);
    save_profile(&instance, "Default", &[("ENB", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // ENB writes a cache file into enbseries/ during play
    let generated = instance.config.game_dir.join("enbseries/cache.bin");
    std::fs::write(&generated, "cache").expect("write");

    purge(&instance, &NullSink).expect("purge");

    // Captured under Root/ so it re-deploys to the game root next time
    let captured = instance.overwrite_dir().join("Root/enbseries/cache.bin");
    assert_eq!(
        std::fs::read_to_string(&captured).expect("captured"),
        "cache"
    );
    assert!(!generated.exists());
}

#[test]
fn higher_priority_mod_wins_conflicts() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "Winner", &[("shared.txt", "winner")]);
    install_mod(&instance, "Loser", &[("shared.txt", "loser")]);
    // Top of the list = highest priority
    save_profile(&instance, "Default", &[("Winner", true), ("Loser", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let path = deployed(&instance, "shared.txt");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "winner");
}

#[test]
fn disabled_mods_are_not_deployed() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "On", "On.esp");
    install_plugin(&instance, "Off", "Off.esp");
    save_profile(&instance, "Default", &[("On", true), ("Off", false)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    assert!(deployed(&instance, "On.esp").exists());
    assert!(
        !deployed(&instance, "Off.esp").exists(),
        "disabled mod must not deploy"
    );
}

#[test]
fn deploy_writes_plugins_txt_and_purge_restores_backup() {
    let (_tmp, instance) = temp_instance();
    let local = instance.config.local_dir.clone().expect("local dir set");
    std::fs::create_dir_all(&local).expect("mk local");
    // An existing Plugins.txt that purge must put back, byte for byte
    std::fs::write(local.join("Plugins.txt"), b"*Original.esp\n").expect("seed");

    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(
        deployment.plugins_txt_backup.as_deref(),
        Some(&b"*Original.esp\n"[..]),
        "the original Plugins.txt is captured in the deployment record"
    );

    // The real Plugins.txt now reflects the deployed, active plugin
    let txt = std::fs::read_to_string(local.join("Plugins.txt")).expect("read");
    assert_eq!(txt, "*Cool.esp\n");

    purge(&instance, &NullSink).expect("purge");

    // Purge restores the user's original file exactly
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        b"*Original.esp\n"
    );
}

#[test]
fn status_is_none_when_nothing_deployed() {
    let (_tmp, instance) = temp_instance();
    assert!(status(&instance).expect("status").is_none());
}

#[test]
fn status_reports_the_live_deployment() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let report = status(&instance).expect("status").expect("deployed");
    assert_eq!(report.deployment.profile, "Default");
    assert!(report.verified.is_complete(), "all deployed files present");
    assert!(
        report
            .deployment
            .record
            .entries
            .iter()
            .any(|e| e.relative == Utf8Path::new("Data").join("Cool.esp"))
    );
}

#[test]
fn status_detects_a_missing_deployed_file() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // Simulate the game dir being tampered with: delete a deployed file
    std::fs::remove_file(deployed(&instance, "Cool.esp")).expect("remove");

    let report = status(&instance).expect("status").expect("deployed");
    assert!(!report.verified.is_complete());
    assert!(
        report
            .verified
            .missing
            .iter()
            .any(|f| *f == Utf8Path::new("Data").join("Cool.esp"))
    );
}

#[test]
fn status_treats_a_replaced_deployed_file_as_present() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "ours")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let dest = deployed(&instance, "Textures/a.dds");
    std::fs::remove_file(&dest).expect("remove our link");
    std::fs::write(&dest, "tool output").expect("replace");

    let report = status(&instance).expect("status").expect("deployed");
    assert!(
        report.verified.is_complete(),
        "status verifies path presence, not hard-link identity"
    );
}

#[test]
fn deploy_rolls_back_created_files_when_save_redirect_fails() {
    let (_tmp, mut instance) = temp_instance();
    deploy_profile_with_local_saves(&instance);
    let bad_ini_dir = instance.root.join("bad-ini-dir");
    std::fs::write(&bad_ini_dir, "not a directory").expect("bad ini dir");
    instance.config.ini_dir = Some(bad_ini_dir);

    let err = deploy_profile(&instance, "Default", &NullSink).expect_err("save redirect must fail");

    assert!(matches!(err, ApplyError::Io(_)));
    assert!(
        !deployed(&instance, "Textures/a.dds").exists(),
        "rollback removes files created before the save redirect failed"
    );
    assert!(
        status(&instance).expect("status").is_none(),
        "rollback clears the deployment journal"
    );
}

#[test]
fn deploy_rolls_back_overwritten_files_when_save_redirect_fails() {
    let (_tmp, mut instance) = temp_instance();
    let data_file = deployed(&instance, "Textures/a.dds");
    std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
    std::fs::write(&data_file, "vanilla").expect("seed vanilla");

    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    let mut profile = Profile::load(&instance, "Default").expect("load");
    profile.local_saves = true;
    profile.save(&instance).expect("save");

    let bad_ini_dir = instance.root.join("bad-ini-dir");
    std::fs::write(&bad_ini_dir, "not a directory").expect("bad ini dir");
    instance.config.ini_dir = Some(bad_ini_dir);

    let err = deploy_profile(&instance, "Default", &NullSink).expect_err("save redirect must fail");

    assert!(matches!(err, ApplyError::Io(_)));
    assert_eq!(
        std::fs::read_to_string(&data_file).expect("read"),
        "vanilla",
        "rollback restores the file that deploy overwrote"
    );
    assert!(
        status(&instance).expect("status").is_none(),
        "rollback clears the deployment journal"
    );
}

#[test]
fn an_interrupted_deployment_is_recovered_so_the_next_deploy_proceeds() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    // Deploy, then forge the journal back to InProgress to mimic a crash that; struck after the files landed but before the commit flip
    deploy_profile(&instance, "Default", &NullSink).expect("first deploy");
    force_status(&instance, Status::InProgress);

    // A non-Committed journal must be reversed on the next entry; without; recovery this second deploy would be refused with AlreadyDeployed
    deploy_profile(&instance, "Default", &NullSink).expect("recovery clears the way");

    assert!(deployed(&instance, "Cool.esp").exists());
    assert_eq!(
        Deployment::load(&instance).expect("load").status,
        Status::Committed
    );
}

#[test]
fn a_held_lock_makes_deploy_busy() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    let _held = InstanceLock::acquire(&instance).expect("hold the lock");
    let err = deploy_profile(&instance, "Default", &NullSink)
        .expect_err("deploy must observe the held lock");
    assert!(matches!(err, ApplyError::Busy));
}

#[test]
fn a_held_lock_makes_purge_busy() {
    let (_tmp, instance) = temp_instance();

    let _held = InstanceLock::acquire(&instance).expect("hold the lock");
    let err = purge(&instance, &NullSink).expect_err("purge must observe the held lock");
    assert!(matches!(err, ApplyError::Busy));
}

#[test]
fn status_reports_an_interrupted_deployment_without_mutating_it() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    // Deploy, then forge the journal back to InProgress to mimic a crash that; struck after the files landed but before the commit flip
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    force_status(&instance, Status::InProgress);

    let live = status(&instance)
        .expect("status")
        .expect("interrupted deployment");
    assert_eq!(live.deployment.status, Status::InProgress);
    assert!(
        Deployment::exists(&instance),
        "status leaves the journal intact"
    );
    assert!(
        deployed(&instance, "Cool.esp").exists(),
        "status does not mutate the game folder"
    );
}

#[test]
fn a_held_lock_makes_status_busy() {
    let (_tmp, instance) = temp_instance();

    let _held = InstanceLock::acquire(&instance).expect("hold the lock");
    let err = status(&instance).expect_err("status must observe the held lock");
    assert!(matches!(err, ApplyError::Busy));
}

#[test]
fn purge_keeps_a_plugins_txt_edited_after_deployment() {
    let (_tmp, instance) = temp_instance();
    let local = instance.config.local_dir.clone().expect("local dir set");
    std::fs::create_dir_all(&local).expect("mk local");
    // The user's original list, which an untouched purge would restore
    std::fs::write(local.join("Plugins.txt"), b"*Original.esp\n").expect("seed original");

    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // A tool or the user rewrites Plugins.txt after deployment
    std::fs::write(local.join("Plugins.txt"), b"*Edited.esp\n").expect("edit after deploy");

    let outcome = purge(&instance, &NullSink).expect("purge");

    // The post-deploy edit is preserved, not rolled back to the original
    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("read"),
        b"*Edited.esp\n"
    );
    assert!(
        !Deployment::exists(&instance),
        "purge still completes and clears the journal on a Plugins.txt conflict"
    );
    assert_eq!(outcome.plugins_txt, Restore::Conflict);
}

#[test]
fn an_orphaned_backup_dir_refuses_deploy() {
    let (_tmp, instance) = temp_instance();
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    // A leftover backup dir means a previous run never finished cleaning up
    let backup_root = instance.config.game_dir.join(".overseer-backup");
    std::fs::create_dir_all(&backup_root).expect("plant orphan backup");

    let err = deploy_profile(&instance, "Default", &NullSink)
        .expect_err("deploy must refuse over an orphaned backup");
    assert!(matches!(err, ApplyError::OrphanedBackup { .. }));
}

#[test]
fn a_reversal_that_cannot_finish_keeps_a_recovery_failed_journal() {
    let (_tmp, instance) = temp_instance();
    // A vanilla file gets backed up on deploy, so a backup dir lives alongside; the deployment until purge restores it
    let data_file = deployed(&instance, "conflict.txt");
    std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
    std::fs::write(&data_file, "vanilla").expect("seed vanilla");
    install_mod(&instance, "Overwriter", &[("conflict.txt", "modded")]);
    save_profile(&instance, "Default", &[("Overwriter", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // Plant a stray file no entry will claim, so the sweep at the end of; reversal reports it as an unresolved residual backup
    let backup_root = instance.config.game_dir.join(".overseer-backup");
    std::fs::write(backup_root.join("stray.bin"), b"junk").expect("plant stray");

    let err = purge(&instance, &NullSink).expect_err("purge cannot fully resolve");
    assert!(matches!(err, ApplyError::RecoveryFailed { .. }));

    // The journal survives, flagged so the next entry point knows to retry
    assert!(Deployment::exists(&instance));
    assert_eq!(
        Deployment::load(&instance).expect("load").status,
        Status::RecoveryFailed
    );
}

/// A RecoveryFailed journal retries and clears once the obstruction (a stray backup file) is removed
#[test]
fn a_recovery_failed_journal_resolves_on_retry_once_the_obstruction_is_cleared() {
    let (_tmp, instance) = temp_instance();
    // A vanilla file is backed up on deploy, so a backup dir lives alongside the deployment
    let data_file = deployed(&instance, "conflict.txt");
    std::fs::create_dir_all(data_file.parent().expect("parent")).expect("mk Data");
    std::fs::write(&data_file, "vanilla").expect("seed vanilla");
    install_mod(&instance, "Overwriter", &[("conflict.txt", "modded")]);
    save_profile(&instance, "Default", &[("Overwriter", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // Plant a stray backup file no entry claims, so the first reversal is left RecoveryFailed
    let backup_root = instance.config.game_dir.join(".overseer-backup");
    std::fs::write(backup_root.join("stray.bin"), b"junk").expect("plant stray");
    purge(&instance, &NullSink).expect_err("first purge cannot fully resolve");
    assert_eq!(
        Deployment::load(&instance).expect("load").status,
        Status::RecoveryFailed
    );

    // The user clears the obstruction as the error instructs, then re-runs a command
    std::fs::remove_file(backup_root.join("stray.bin")).expect("clear the obstruction");

    purge(&instance, &NullSink).expect("purge retries recovery");
    assert!(
        !Deployment::exists(&instance),
        "the journal is cleared on a resolved retry"
    );
    assert_eq!(
        std::fs::read_to_string(&data_file).expect("read"),
        "vanilla",
        "the vanilla original stays restored across the retry"
    );
    assert!(
        !backup_root.exists(),
        "the emptied backup root is swept away"
    );
}

// --- per-profile saves ---

/// `Fallout4Custom.ini` under the instance's (temp) My Games dir
fn custom_ini(instance: &Instance) -> Utf8PathBuf {
    let stem = instance.config.game.ini_stem();
    instance
        .ini_dir()
        .expect("ini dir")
        .join(format!("{stem}Custom.ini"))
}

/// The live `SLocalSavePath` value, if any
fn save_path(instance: &Instance) -> Option<String> {
    let text = std::fs::read_to_string(custom_ini(instance)).ok()?;
    crate::ini::Ini::parse(&text)
        .get("General", "SLocalSavePath")
        .map(str::to_owned)
}

/// Save `Default` with a single enabled mod and per-profile saves switched on
fn deploy_profile_with_local_saves(instance: &Instance) {
    install_mod(instance, "CoolMod", &[("Textures/a.dds", "pix")]);
    save_profile(instance, "Default", &[("CoolMod", true)]);
    let mut profile = Profile::load(instance, "Default").expect("load");
    profile.local_saves = true;
    profile.save(instance).expect("save");
}

#[test]
fn deploy_with_local_saves_redirects_saves_and_purge_restores() {
    let (_tmp, instance) = temp_instance();
    deploy_profile_with_local_saves(&instance);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    assert_eq!(
        save_path(&instance).as_deref(),
        Some("Saves\\Default\\"),
        "redirect written into Fallout4Custom.ini"
    );
    assert!(
        instance
            .ini_dir()
            .unwrap()
            .join("Saves")
            .join("Default")
            .is_dir(),
        "the profile's saves folder is pre-created"
    );

    // Nothing to put back: the user had no prior value
    let journal = Deployment::load(&instance).expect("journal");
    assert_eq!(
        journal.save_redirect.expect("redirect journalled").original,
        None
    );

    let outcome = purge(&instance, &NullSink).expect("purge");
    assert_eq!(save_path(&instance), None, "our redirect removed on purge");
    assert_eq!(outcome.save_redirect, Restore::Restored);
}

#[test]
fn deploy_without_local_saves_never_touches_saves() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pix")]);
    // save_profile leaves local_saves off
    save_profile(&instance, "Default", &[("CoolMod", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    assert!(
        Deployment::load(&instance)
            .expect("journal")
            .save_redirect
            .is_none(),
        "no redirect journalled when the profile opts out"
    );
    assert!(
        !custom_ini(&instance).exists(),
        "the custom INI is never created"
    );
}

#[test]
fn deploy_captures_the_users_prior_save_path_and_purge_restores_it() {
    let (_tmp, instance) = temp_instance();
    deploy_profile_with_local_saves(&instance);

    // The user already had a custom save path
    let ini = custom_ini(&instance);
    std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
    std::fs::write(&ini, "[General]\r\nSLocalSavePath=Saves\\Mine\\\r\n").unwrap();

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    assert_eq!(
        Deployment::load(&instance)
            .unwrap()
            .save_redirect
            .expect("journalled")
            .original
            .as_deref(),
        Some("Saves\\Mine\\"),
        "the prior value is captured"
    );
    assert_eq!(save_path(&instance).as_deref(), Some("Saves\\Default\\"));

    let outcome = purge(&instance, &NullSink).expect("purge");
    assert_eq!(
        save_path(&instance).as_deref(),
        Some("Saves\\Mine\\"),
        "the user's original save path is restored on purge"
    );
    assert_eq!(outcome.save_redirect, Restore::Restored);
}

#[test]
fn purge_keeps_a_save_path_changed_after_deployment() {
    let (_tmp, instance) = temp_instance();
    deploy_profile_with_local_saves(&instance);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    // The user re-points their save path while deployed
    std::fs::write(
        custom_ini(&instance),
        "[General]\r\nSLocalSavePath=Saves\\Manual\\\r\n",
    )
    .unwrap();

    let outcome = purge(&instance, &NullSink).expect("purge");
    assert_eq!(
        save_path(&instance).as_deref(),
        Some("Saves\\Manual\\"),
        "a value the user changed after deploy is left alone"
    );
    assert!(
        !Deployment::exists(&instance),
        "purge still completes and clears the journal"
    );
    assert_eq!(outcome.save_redirect, Restore::Conflict);
}

#[test]
fn purge_captures_a_foreign_replacement_before_restoring_its_backup() {
    let (_tmp, instance) = temp_instance();
    let dest = deployed(&instance, "Textures/a.dds");
    std::fs::create_dir_all(dest.parent().expect("parent")).expect("Data tree");
    std::fs::write(&dest, "vanilla").expect("seed original");
    install_mod(&instance, "Texturer", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Texturer", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    std::fs::remove_file(&dest).expect("remove deployed link");
    std::fs::write(&dest, "tool output").expect("replace destination");
    let outcome = purge(&instance, &NullSink).expect("purge");

    assert_eq!(
        std::fs::read_to_string(&dest).expect("restored original"),
        "vanilla"
    );
    assert_eq!(
        std::fs::read_to_string(instance.overwrite_dir().join("Textures/a.dds"))
            .expect("captured replacement"),
        "tool output"
    );
    assert_eq!(outcome.restored.len(), 1);
    assert_eq!(outcome.captured.len(), 1);
}

#[test]
fn purge_leaves_preexisting_unrecorded_files_untouched() {
    let (_tmp, instance) = temp_instance();
    let vanilla = deployed(&instance, "Loose/vanilla.txt");
    std::fs::create_dir_all(vanilla.parent().expect("parent")).expect("Data tree");
    std::fs::write(&vanilla, "vanilla").expect("seed vanilla");
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let outcome = purge(&instance, &NullSink).expect("purge");

    assert_eq!(
        std::fs::read_to_string(&vanilla).expect("vanilla remains"),
        "vanilla"
    );
    assert!(outcome.captured.is_empty());
    assert!(!instance.overwrite_dir().join("Loose/vanilla.txt").exists());
}

#[test]
fn legacy_capture_remains_limited_to_recorded_created_directories() {
    let (_tmp, instance) = temp_instance();
    std::fs::create_dir_all(instance.config.game_dir.join("Data")).expect("preexisting Data");
    install_mod(&instance, "Mod", &[("Generated/Nested/mod.bin", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    Deployment::remove_baseline(&instance).expect("simulate legacy journal");

    let covered = deployed(&instance, "Generated/Nested/output.log");
    std::fs::write(&covered, "covered").expect("covered residue");
    let outside = deployed(&instance, "outside.log");
    std::fs::write(&outside, "outside").expect("outside residue");

    let outcome = purge(&instance, &NullSink).expect("legacy purge");

    assert_eq!(
        std::fs::read_to_string(instance.overwrite_dir().join("Generated/Nested/output.log"))
            .expect("captured residue"),
        "covered"
    );
    assert_eq!(
        std::fs::read_to_string(&outside).expect("outside remains"),
        "outside"
    );
    assert_eq!(outcome.captured.len(), 1);
}

#[test]
fn legacy_foreign_target_without_backup_is_preserved_nonblocking() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    Deployment::remove_baseline(&instance).expect("simulate legacy journal");
    let dest = deployed(&instance, "Textures/a.dds");
    std::fs::remove_file(&dest).expect("remove link");
    std::fs::write(&dest, "foreign").expect("replace");

    let outcome = purge(&instance, &NullSink).expect("legacy purge");

    assert_eq!(
        std::fs::read_to_string(&dest).expect("foreign remains"),
        "foreign"
    );
    assert_eq!(outcome.preserved_conflicts.len(), 1);
    assert!(!outcome.preserved_conflicts[0].blocking);
    assert!(!Deployment::exists(&instance));
}

#[test]
fn matching_pending_and_overwrite_content_resolves_capture_retry() {
    let (_tmp, instance) = temp_instance();
    let pending = instance
        .config
        .game_dir
        .join(".overseer-backup/.capture/Logs/output.log");
    let overwrite = instance.overwrite_dir();
    let destination = overwrite.join("Logs/output.log");
    std::fs::create_dir_all(pending.parent().expect("pending parent")).expect("pending tree");
    std::fs::create_dir_all(destination.parent().expect("overwrite parent"))
        .expect("overwrite tree");
    std::fs::write(&pending, "same").expect("pending");
    std::fs::write(&destination, "same").expect("delivered");
    let mut outcome = ReversalOutcome::default();

    deliver_pending(
        &pending,
        Utf8Path::new("Data/Logs/output.log"),
        Utf8Path::new("Logs/output.log"),
        &overwrite,
        &mut outcome,
    );

    assert!(!pending.exists(), "matching pending duplicate is removed");
    assert_eq!(outcome.captured.len(), 1);
    assert!(outcome.preserved_conflicts.is_empty());
}

#[test]
fn missing_staging_source_keeps_destination_backup_and_journal() {
    let (_tmp, instance) = temp_instance();
    let dest = deployed(&instance, "conflict.txt");
    std::fs::create_dir_all(dest.parent().expect("parent")).expect("Data tree");
    std::fs::write(&dest, "vanilla").expect("seed original");
    install_mod(&instance, "Mod", &[("conflict.txt", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let source = deployment.record.entries[0].source.clone();
    std::fs::remove_file(&source).expect("remove source");

    let error = purge(&instance, &NullSink).expect_err("ownership is unknown");
    let ApplyError::RecoveryFailed { outcome, .. } = error else {
        panic!("incomplete purge returns its structured outcome")
    };

    assert!(!outcome.unresolved.is_empty());
    assert_eq!(
        std::fs::read_to_string(&dest).expect("destination preserved"),
        "modded"
    );
    assert_eq!(
        std::fs::read_to_string(
            instance
                .config
                .game_dir
                .join(".overseer-backup/Data/conflict.txt")
        )
        .expect("backup preserved"),
        "vanilla"
    );
    assert_eq!(
        Deployment::load(&instance).expect("journal").status,
        Status::RecoveryFailed
    );
}

#[test]
fn capture_collision_preserves_both_copies_and_keeps_the_journal() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let dest = deployed(&instance, "Textures/a.dds");
    std::fs::remove_file(&dest).expect("remove deployed link");
    std::fs::write(&dest, "new output").expect("tool output");
    let overwrite = instance.overwrite_dir().join("Textures/a.dds");
    std::fs::create_dir_all(overwrite.parent().expect("parent")).expect("overwrite tree");
    std::fs::write(&overwrite, "prior output").expect("prior output");

    let error = purge(&instance, &NullSink).expect_err("collision blocks completion");
    let ApplyError::RecoveryFailed { outcome, .. } = error else {
        panic!("collision returns a recovery outcome")
    };
    let pending = instance
        .config
        .game_dir
        .join(".overseer-backup/.capture/Textures/a.dds");

    assert_eq!(
        std::fs::read_to_string(&overwrite).expect("prior output"),
        "prior output"
    );
    assert_eq!(
        std::fs::read_to_string(&pending).expect("pending output"),
        "new output"
    );
    assert!(
        outcome
            .preserved_conflicts
            .iter()
            .any(|conflict| conflict.blocking)
    );
    assert!(Deployment::exists(&instance));
}

#[test]
fn deploy_aborts_when_interrupted_cleanup_cannot_resolve_ownership() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    let deployment = deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    force_status(&instance, Status::InProgress);
    std::fs::remove_file(&deployment.record.entries[0].source).expect("remove source");

    let error =
        deploy_profile(&instance, "Default", &NullSink).expect_err("restart must abort safely");

    assert!(matches!(error, ApplyError::RecoveryFailed { .. }));
    let failed = Deployment::load(&instance).expect("journal");
    assert_eq!(
        (failed.status, failed.committed),
        (Status::RecoveryFailed, Some(false))
    );
    assert!(!failed.was_committed());
    assert!(
        deployed(&instance, "Textures/a.dds").exists(),
        "unverifiable destination is preserved"
    );
}

#[test]
fn interrupted_origin_remains_unconditional_after_recovery_failed() {
    let (_tmp, instance) = temp_instance();
    let local = instance.config.local_dir.clone().expect("local dir");
    std::fs::create_dir_all(&local).expect("local dir");
    let plugins_txt = local.join("Plugins.txt");
    let original = b"*Original.esp\n";
    std::fs::write(&plugins_txt, original).expect("seed original");
    install_plugin(&instance, "CoolMod", "Cool.esp");
    save_profile(&instance, "Default", &[("CoolMod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    let deployed = std::fs::read(&plugins_txt).expect("deployed Plugins.txt");
    assert_ne!(deployed, original);

    let mut interrupted = Deployment::load(&instance).expect("journal");
    interrupted.status = Status::InProgress;
    interrupted.committed = Some(false);
    interrupted.plugins_txt_intended = None;
    interrupted.save(&instance).expect("simulate crash window");

    let held = local.join("Plugins.deployed");
    std::fs::rename(&plugins_txt, &held).expect("hold deployed bytes");
    std::fs::create_dir(&plugins_txt).expect("block Plugins.txt restore");

    let error = purge(&instance, &NullSink).expect_err("first restore is blocked");
    let ApplyError::RecoveryFailed { outcome, .. } = error else {
        panic!("blocked restore keeps structured recovery state")
    };
    assert!(
        outcome
            .unresolved
            .iter()
            .any(|issue| issue.path == plugins_txt)
    );

    let failed = Deployment::load(&instance).expect("RecoveryFailed journal");
    assert_eq!(
        (failed.status, failed.committed),
        (Status::RecoveryFailed, Some(false))
    );
    assert!(!failed.was_committed());
    assert_eq!(failed.plugins_txt_backup.as_deref(), Some(&original[..]));
    assert_eq!(
        std::fs::read(&held).expect("deployed bytes remain available"),
        deployed
    );

    std::fs::remove_dir(&plugins_txt).expect("clear obstruction");
    std::fs::rename(&held, &plugins_txt).expect("restore deployed path");
    let outcome = purge(&instance, &NullSink).expect("retry purge");

    assert_eq!(outcome.plugins_txt, Restore::Restored);
    assert_eq!(
        std::fs::read(&plugins_txt).expect("original restored"),
        original
    );
    assert!(!Deployment::exists(&instance));
}

#[test]
fn interrupted_cleanup_restores_plugin_and_save_originals_unconditionally() {
    let (_tmp, instance) = temp_instance();
    deploy_profile_with_local_saves(&instance);
    install_plugin(&instance, "CoolMod", "Cool.esp");
    let local = instance.config.local_dir.clone().expect("local dir");
    std::fs::create_dir_all(&local).expect("local dir");
    std::fs::write(local.join("Plugins.txt"), b"*Original.esp\n").expect("plugins original");
    let ini = custom_ini(&instance);
    std::fs::create_dir_all(ini.parent().expect("INI parent")).expect("INI parent");
    std::fs::write(&ini, "[General]\r\nSLocalSavePath=Saves\\Mine\\\r\n").expect("save original");

    deploy_profile(&instance, "Default", &NullSink).expect("deploy");
    force_status(&instance, Status::InProgress);
    let outcome = purge(&instance, &NullSink).expect("interrupted purge");

    assert_eq!(
        std::fs::read(local.join("Plugins.txt")).expect("plugins restored"),
        b"*Original.esp\n"
    );
    assert_eq!(save_path(&instance).as_deref(), Some("Saves\\Mine\\"));
    assert_eq!(outcome.plugins_txt, Restore::Restored);
    assert_eq!(outcome.save_redirect, Restore::Restored);
}

#[test]
fn case_only_baseline_difference_does_not_recapture_a_vanilla_file() {
    let (_tmp, instance) = temp_instance();
    let vanilla = deployed(&instance, "Loose/Case.log");
    std::fs::create_dir_all(vanilla.parent().expect("parent")).expect("Data tree");
    std::fs::write(&vanilla, "vanilla").expect("seed vanilla");
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let mut baseline = Deployment::load_baseline(&instance)
        .expect("load baseline")
        .expect("new journal has baseline");
    let entry = baseline
        .iter_mut()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("Case.log"))
        })
        .expect("vanilla baseline entry");
    *entry = Utf8PathBuf::from(entry.as_str().to_uppercase());
    Deployment::save_baseline(&instance, &baseline).expect("rewrite baseline case");

    let outcome = purge(&instance, &NullSink).expect("purge");

    assert_eq!(
        std::fs::read_to_string(&vanilla).expect("vanilla remains"),
        "vanilla"
    );
    assert!(outcome.captured.is_empty());
}

#[cfg(windows)]
#[test]
fn new_junction_is_preserved_without_traversal_or_capture() {
    let (_tmp, instance) = temp_instance();
    install_mod(&instance, "Mod", &[("Textures/a.dds", "modded")]);
    save_profile(&instance, "Default", &[("Mod", true)]);
    deploy_profile(&instance, "Default", &NullSink).expect("deploy");

    let target = instance.root.join("junction-target");
    std::fs::create_dir_all(&target).expect("junction target");
    std::fs::write(target.join("secret.txt"), "secret").expect("target content");
    let junction = instance.config.game_dir.join("Data/generated");
    let junction_arg = junction.as_str().replace('/', "\\");
    let target_arg = target.as_str().replace('/', "\\");
    let result = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J", &junction_arg, &target_arg])
        .status()
        .expect("run mklink");
    assert!(result.success(), "create junction");

    let error = purge(&instance, &NullSink).expect_err("new reparse point blocks completion");
    assert!(matches!(error, ApplyError::RecoveryFailed { .. }));
    assert!(junction.symlink_metadata().is_ok(), "junction is preserved");
    assert_eq!(
        std::fs::read_to_string(target.join("secret.txt")).expect("target remains"),
        "secret"
    );
    assert!(
        !instance
            .overwrite_dir()
            .join("generated/secret.txt")
            .exists(),
        "capture never traverses the junction"
    );
}
